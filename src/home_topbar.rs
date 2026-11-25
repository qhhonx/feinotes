//! The header of 主页 and every folder page: one row, the same on each level — the path on the
//! left, five glyphs on the right (搜索 · 批量管理 · 显示 · … · "+").

use super::{HomeSort, HomeTab, SortDir};
use crate::app::Workspace;
use crate::design::{Align, Button, ButtonSize, MenuItem, MenuLabel, MenuSeparator, Popover, hang};
use crate::settings::tr;
use crate::store::Entry;
use crate::theme::Theme;
use crate::ui::{TipExt, TitleBarExt};
use gpui::{prelude::FluentBuilder, *};
use std::rc::Rc;

/// The `RowMenu::from` of a folder page's header "…".
pub(crate) const TB_MORE: &str = "tb-more";

impl Workspace {
    pub(crate) fn dismiss_library_view_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) || self.search_open || self.confirm.is_some() {
            return false;
        }
        if !std::mem::take(&mut self.home.view_menu) { return false; }
        cx.notify();
        true
    }

    /// What a page lists: 主页's level or a folder's, or, with 只看收藏 on, every starred
    /// document below it, flat (主页: the whole library), as 收藏 always listed them.
    pub(crate) fn page_entries(&self, folder: Option<&str>) -> Rc<Vec<Entry>> {
        let Some(id) = folder else { return self.home_entries() };
        if self.home.tab == HomeTab::Recent { return self.level_entries(Some(id)) }
        Rc::new(self.sorted_entries(self.entries.iter()
            .filter(|e| e.meta.folder.is_none() && e.meta.starred > 0 && self.ancestors(&e.id).iter().any(|a| a.id == id)).cloned().collect()))
    }

    fn main_w(&self, window: &Window) -> f32 {
        f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. }
    }

    /// 主页 alone, or a folder's path as its page shows it (`folder_crumbs`); `fold`: a tight header folds the middle.
    fn topbar_crumbs(&self, folder: Option<&str>, fold: bool, cx: &mut Context<Self>) -> Div {
        match folder {
            Some(id) => self.folder_crumbs(id, fold, cx),
            None => div().min_w_0().flex().items_center().text_size(px(crate::ui::CRUMB_SIZE)).line_height(px(20.))
                .child(div().px(px(4.)).whitespace_nowrap().text_color(Theme::text_caption()).child(crate::settings::label(crate::settings::text("主页")))),
        }
    }

    /// The header of 主页 (`folder` None) or a folder's page; in 批量管理 the whole row is its toolbar.
    /// The same five glyphs on both: 搜索 · 批量管理 · 显示 · … · "+". 主页's "+" opens a folder, a
    /// folder's makes or imports; a folder's "…" and "+" are its sidebar row's, 主页's "+" is 我的文档's.
    pub(crate) fn topbar(&self, folder: Option<&str>, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let main = self.main_w(window);
        if let Some(sel) = &self.batch { return self.batch_bar(folder, sel.len(), main, cx) }
        let left = div().relative().min_w_0().overflow_hidden().flex().items_center().child(self.topbar_crumbs(folder, main < 640., cx));
        let icon_btn = crate::ui::header_icon;
        // Menu state at render: a popover's outside press clears it on mouse-down, before a click would fire.
        let view_open = self.home.view_menu;
        let search = icon_btn("tb-search", "search")
            .tip(tr("搜索文档", "Search documents"), Some(crate::shortcuts::hint("workspace::FocusSearch", "⌘ + J")))
            .on_click(cx.listener(|w, _, window, cx| w.open_search(window, cx)));
        let batch = icon_btn("tb-batch", "batch").relative().child(self.home.track("tb-batch")).tip(crate::settings::text("批量管理"), None)
            .on_click(cx.listener(|w, _, _, cx| { w.batch = Some(vec![]); w.row_menu = None; w.home.view_menu = false; cx.notify(); }));
        let display = icon_btn("home-view", "view-settings").relative().selected(view_open).child(self.home.track("home-view"))
            .when(!view_open, |d| d.tip(tr("显示", "View"), None))
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, window, cx| {
                cx.stop_propagation();
                if !view_open { window.focus(&w.focus, cx); }
                w.home.view_menu = !view_open;
                cx.notify();
            }));
        let more = match folder {
            // 主页's "…": the other ways in from this computer (打开文件夹 is its own button).
            None => self.place_button("home-more", cx),
            // A folder's "…": its sidebar row's menu (`folder_row_menu`), as its "+" is the row's "+".
            Some(id) => {
                let (id, open) = (id.to_string(), self.row_menu.as_ref().is_some_and(|m| m.from.as_deref() == Some(TB_MORE)));
                icon_btn("tb-more", "ellipsis").relative().selected(open).child(self.home.track("tb-more"))
                    .when(!open, |d| d.tip(tr("更多", "More"), None))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, _, cx| {
                        cx.stop_propagation();
                        let Some(b) = w.home.anchor("tb-more") else { return };
                        let (pos, corner) = hang(b, Align::End, None);
                        w.row_menu = (!open).then(|| crate::app::RowMenu { id: id.clone(), pos, corner, sub: false, batch: false, from: Some(TB_MORE.into()) });
                        cx.notify();
                    }))
            }
        };
        // The level's main action, last and in sight: on 主页, 打开文件夹 (a folder hung under 主页, used
        // where it is); in a folder, "+" — the 新建 menu every "+" opens, 导入 at its end.
        // One "+" on both, the same glyph in the same place; what its menu holds is the level's: 主页
        // only opens (打开文件夹), a folder makes and imports (`render_create_menu`).
        let primary = {
            let (key, id, parent) = match folder { None => ("create-home", "tb-home-new", None), Some(f) => ("create-folder", "folder-new", Some(f.to_string())) };
            let open = self.create_menu.as_ref().is_some_and(|(_, pos)| self.home.anchor(key).is_some_and(|b| hang(b, Align::End, None).0 == *pos));
            let tip = if folder.is_none() { (tr("打开文件夹", "Open a folder"), crate::shortcuts::hint("workspace::OpenFolder", "⌘ + Shift + O")) } else { (tr("新建", "New"), crate::shortcuts::hint("workspace::NewDoc", "⌘ + N")) };
            icon_btn(id, "plus").relative().selected(open).child(self.home.track(key))
                .when(!open, |d| d.tip(tip.0, Some(tip.1)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |w, _: &ClickEvent, _, cx| { let Some(b) = w.home.anchor(key) else { return }; let pos = hang(b, Align::End, None).0; w.create_menu = if w.create_menu.is_some() { None } else { Some((parent.clone(), pos)) }; cx.notify(); }))
        };
        let right = div().relative().flex_none().flex().items_center().gap(px(2.))
            .child(search).child(batch).child(display).child(more).child(primary).child(self.home.track("tb-right"));
        crate::trash::page_header_with(self, left, 18., cx).child(div().w(px(16.)).flex_none()).child(right).into_any_element()
    }

    /// 批量管理's row: 全选 and the count where the path was, the actions where the controls were.
    fn batch_bar(&self, folder: Option<&str>, n: usize, main: f32, cx: &mut Context<Self>) -> AnyElement {
        use crate::sidebar::{BATCH_LIMIT, Check, checkbox};
        let ids: Vec<String> = self.page_entries(folder).iter().take(BATCH_LIMIT).map(|e| e.id.clone()).collect();
        let all = n > 0 && ids.iter().all(|id| self.batch.as_ref().is_some_and(|s| s.contains(id)));
        let compact = main < 640.;
        let left = div().flex().items_center().min_w_0()
            .child(div().id("home-batch-all").no_drag().flex_none().ml(px(4.)).size(px(14.)).cursor_pointer().group("home-batch-all")
                .child(checkbox(if all { Check::On } else if n > 0 { Check::Partial } else { Check::Off }, false))
                .tip(crate::settings::text("全选"), None)
                .on_click(cx.listener(move |w, _, _, cx| { w.batch = Some(if all { vec![] } else { ids.clone() }); cx.notify(); })))
            .child(div().ml(px(10.)).text_size(px(15.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).whitespace_nowrap()
                .child(crate::localized_format!("已选 {n} 项", "Selected: {n}")));
        let act = |id: &'static str, ic: &'static str, label: &'static str| Button::ghost(id).icon(ic).no_drag().disabled(n == 0)
            .when(!compact, |d| d.label(crate::settings::label(label))).when(compact, |d| d.size(px(crate::app::HEADER_ACTIONS_H)).tip(label, None));
        let right = div().flex_none().flex().items_center().gap(px(2.))
            .child(act("home-batch-copy", "copy", crate::settings::text("创建副本")).when(n > 0, |d| d.on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx)))))
            .child(act("home-batch-move", "folder-input", crate::settings::text("移动到")).when(n > 0, |d| d.on_click(cx.listener(|w, _, window, cx| w.batch_move(window, cx)))))
            .child(act("home-batch-delete", "bin", crate::settings::text("删除")).when(n > 0, |d| d.on_click(cx.listener(|w, _, _, cx| w.ask_batch_delete(cx)))))
            .child(Button::secondary("home-batch-exit").sized(ButtonSize::Sm).no_drag().ml(px(6.)).label(tr("完成", "Done")).relative().child(self.home.track("home-batch-exit"))
                .tip(crate::settings::text("退出批量操作"), None)
                .on_click(cx.listener(|w, _, _, cx| { w.batch = None; w.row_menu = None; cx.notify(); })));
        crate::trash::page_header_with(self, left, 18., cx).child(right).into_any_element()
    }

    /// The sort rows, shared by the sort dropdown and 显示: the orders, then 升序 / 降序 for
    /// the automatic ones, then 文件夹优先.
    pub(crate) fn sort_rows(&self, mut menu: Popover, cx: &mut Context<Self>) -> Popover {
        for (index, order) in [HomeSort::Manual, HomeSort::Visited, HomeSort::Modified, HomeSort::Created, HomeSort::Title].into_iter().enumerate() {
            let g = SharedString::from(format!("home-order-{index}"));
            menu = menu.child(MenuItem::new(g).label(crate::settings::label(order.label())).checked(self.home.sort == order)
                .on_click(cx.listener(move |w, _, _, cx| { w.home.view_menu = false; w.sort_by(order, cx) })));
        }
        if self.home.sort.directional() {
            menu = menu.child(MenuSeparator::new());
            for (index, dir) in [SortDir::Asc, SortDir::Desc].into_iter().enumerate() {
                menu = menu.child(MenuItem::new(SharedString::from(format!("home-dir-{index}"))).label(dir.label()).checked(self.home.sort_dir == dir)
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.home.sort_dir = dir;
                        w.save_home();
                       
                        w.home.view_menu = false;
                        w.home.grid_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        w.home.folder_grid_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        w.home.list_scroll.set_offset(point(px(0.), px(0.)));
                        w.home.folder_list_scroll.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    })));
            }
        }
        menu.child(MenuSeparator::new()).child(MenuItem::new("home-folders-first").label(tr("文件夹优先", "Folders first")).checked(self.home.folders_first && self.home.sort != HomeSort::Manual)
            .when(self.home.sort == HomeSort::Manual, |d| d.opacity(0.4))
            .on_click(cx.listener(|w, _, _, cx| { if w.home.sort == HomeSort::Manual { return; } w.home.folders_first = !w.home.folders_first; w.save_home(); w.home.view_menu = false; cx.notify(); })))
    }

    /// 显示 (Finder's and Notion's view options): 视图 and 排序 in one menu; 只看收藏 stays out, a star of its own.
    pub(crate) fn view_menu(&self, cx: &mut Context<Self>) -> Popover {
        let view = |id: &'static str, ic: &'static str, label: &'static str, grid: bool| MenuItem::new(id).icon(ic).label(label).checked(self.home.grid == grid)
            .on_click(cx.listener(move |w, _, _, cx| { w.home.grid = grid; w.save_home(); w.home.view_menu = false; cx.notify(); }));
        let menu = Popover::new("view-menu").menu().min_w(px(220.))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.home.view_menu = false; cx.notify(); }))
            .child(MenuLabel::new(tr("视图", "View")))
            .child(view("tb-grid", "view-grid", tr("宫格", "Grid"), true))
            .child(view("tb-list", "view-list", tr("列表", "List"), false))
            .child(MenuSeparator::new())
            .child(MenuLabel::new(tr("排序", "Sort by")));
        self.sort_rows(menu, cx)
    }
}
