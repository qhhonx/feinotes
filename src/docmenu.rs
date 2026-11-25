//! The header "…" menu: how the document is seen (page width, then the 阅读模式, 显示引用列表
//! and 窗口保持在最前 switches), find & replace, move, pin, star, 新建子文档, duplicate,
//! 下载为 PDF / Markdown, 复制为 Markdown, 打印, document info, 版本历史, delete, 快捷键.

use crate::app::Workspace;
use crate::settings::display_title;
use crate::store::PageWidth;
use crate::design::{MenuItem, MenuSeparator, Popover, menu};
use crate::ui::TipExt;
use gpui::{prelude::FluentBuilder, *};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DocSub {
    Width,
    Download,
}

const MENU_W: f32 = 200.;
/// The menu's padding (`design::menu`): a submenu's first item sits this far below its top.
const PAD: f32 = menu::PAD;

thread_local! {
    /// Where the rows with a submenu were last painted, so the submenu levels with its row wherever
    /// the row is: no counting rows and dividers above it, and right after the menu scrolls.
    static ROWS: std::cell::RefCell<std::collections::HashMap<&'static str, Bounds<Pixels>>> = Default::default();
}

/// Records where the row `key` is painted (a submenu's row).
fn record_row(key: &'static str) -> impl IntoElement {
    canvas(move |b, _, _| ROWS.with(|m| { m.borrow_mut().insert(key, b); }), |_, _, _, _| {}).absolute().inset_0()
}

/// The top of the row `key` as last painted.
fn row_top(key: &'static str) -> Option<f32> { ROWS.with(|m| m.borrow().get(key).map(|b| f32::from(b.top()))) }

/// A row; one whose submenu is open stays lit (`open`).
fn menu_item(id: &'static str, ic: &'static str, label: impl Into<SharedString>, sub: bool, open: bool) -> MenuItem {
    MenuItem::new(id).icon(ic).label(crate::settings::label(label.into())).submenu(sub).open(open)
}

/// 下载为's text-only items.
pub(crate) fn sub_item(id: &'static str, label: &'static str) -> MenuItem {
    MenuItem::new(id).label(crate::settings::label(label))
}

fn divider() -> MenuSeparator { MenuSeparator::new() }

impl Workspace {
    fn close_doc_menu(&mut self, cx: &mut Context<Self>) {
        self.doc_menu = false;
        self.doc_sub = None;
        cx.notify();
    }

    pub fn set_page_width(&mut self, width: PageWidth, cx: &mut Context<Self>) {
        let Some(id) = self.current.clone() else { return };
        if let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) {
            e.meta.width = width;
        }
        let _ = self.store.set_width(&id, width);
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| {
                e.page_width = width;
                cx.notify();
            });
        }
        cx.notify();
    }

    /// The document as it stands: the open editor's copy, or the file on disk.
    pub(crate) fn doc_of(&mut self, id: &str, cx: &mut Context<Self>) -> Option<crate::model::Document> {
        if self.current.as_deref() == Some(id) {
            self.flush(cx);
            return self.editor.as_ref().map(|ed| ed.read(cx).core.doc.clone());
        }
        self.store.load(id).ok()
    }

    /// 创建副本: "<标题> 副本", opened right away.
    pub fn duplicate_doc(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let file_before = self.store.track_file_changes();
        self.close_doc_menu(cx);
        self.row_menu = None;
        if let Some(new) = self.copy_doc(id, cx) {
            self.entries.set(self.store.list());
            self.sort();
            self.open(&new, window, cx);
        }
        self.remember_files(file_before, crate::settings::tr("创建副本", "Duplicate"));
        cx.notify();
    }

    /// Write "<标题> 副本" of `id` to the library; the caller refreshes the list.
    pub(crate) fn copy_doc(&mut self, id: &str, cx: &mut Context<Self>) -> Option<String> {
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) { return self.copy_folder(id, cx); }
        let mut doc = self.doc_of(id, cx)?;
        let title = crate::localized_format!("{} 副本", "{} copy", display_title(&doc.title.text));
        doc.title = crate::model::RichText::plain(title);
        let width = self.entries.iter().find(|e| e.id == id).map(|e| e.meta.width).unwrap_or_default();
        // A Markdown file copies as Markdown, into the library even from a 本机文件.
        // A 画板 copies as a 画板.
        let board = self.entries.iter().find(|e| e.id == id).filter(|e| e.meta.kind == crate::store::DocKind::Board)
            .map(|_| doc.blocks.iter().find_map(|b| match &b.kind { crate::model::BlockKind::Board(bd) => Some(bd.clone()), _ => None }).unwrap_or_default());
        let table = self.entries.iter().any(|e| e.id == id && e.meta.kind == crate::store::DocKind::Table);
        if table { crate::table::flush_all(cx); }
        let made = if table { self.store.copy_table(id, &doc.title.text) }
            else if let Some(board) = board { self.store.create_board_in(&doc.title.text, &board, None) }
            else if self.store.is_markdown(id) { let front = self.store.front_matter_of(id); self.store.create_markdown(&doc, &front) } else { self.store.create_from(&doc) };
        match made {
            Ok(new) => {
                let _ = self.store.set_width(&new, width);
                // A document with children is copied alone, beside its folder.
                let row = self.row_of(id);
                let parent = self.entries.iter().find(|e| e.id == row).filter(|e| !e.meta.local()).and_then(|e| e.meta.parent.clone());
                if let Some(parent) = parent { let _ = self.store.set_parent(&new, Some(&parent)); }
                Some(new)
            }
            Err(e) => {
                self.status = crate::app::SaveStatus::Failed(crate::localized_format!("创建副本失败：{}", "Could not duplicate document: {}", crate::settings::diagnostic(&e.to_string())));
                None
            }
        }
    }

    /// 查找和替换 (also ⌘F / ⌘⇧H).
    pub fn find_in_doc(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_doc_menu(cx);
        // the document's text is hidden under an open board
        if self.editor.as_ref().is_some_and(|e| e.read(cx).board_open()) { return; }
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| e.open_find(replace, window, cx));
        }
    }

    pub(crate) fn render_doc_menu(&self, _window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !self.doc_menu {
            return vec![];
        }
        let Some(id) = self.current.clone() else { return vec![] };
        let entry = self.entries.iter().find(|e| e.id == id);
        let pinned = entry.is_some_and(|e| e.meta.pinned);
        let starred = entry.is_some_and(|e| e.meta.starred > 0);
        let width = entry.map(|e| e.meta.width).unwrap_or_default();
        let refs_shown = !entry.is_some_and(|e| e.meta.refs_hidden);
        // An open board isn't the page: no page width, find in its text or
        // editor shortcuts; a standalone board file has no Markdown or
        // references either
        let (board, board_file) = self.editor.as_ref().map_or((false, false), |e| { let e = e.read(cx); (e.board_open(), e.board_file) });
        let reading = self.reading();
        let on_top = self.on_top;
        let sub = self.doc_sub;
        let hover_sub = |s: Option<DocSub>| {
            cx.listener(move |w: &mut Workspace, hovered: &bool, _, cx| {
                if *hovered && w.doc_sub != s {
                    w.doc_sub = s;
                    cx.notify();
                }
            })
        };
        let sub_bounds = self.doc_sub_bounds.clone();
        // The menu follows its longest translation (#225); its rendered width
        // (from the previous frame — a submenu only opens after one) places
        // the submenus that open to its left.
        let menu_w_record = self.doc_menu_w.clone();
        let menu_w = move || {
            let record = menu_w_record.clone();
            canvas(move |b, _, _| record.set(Some(f32::from(b.size.width))), |_, _, _, _| {}).absolute().inset_0()
        };
        let (pin_id, star_id, mv_id, del_id, dup_id, print_id) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
        // 新建子文档 (#357): under a document of the library.
        let child = !board_file && self.takes_children(&id);
        let child_id = id.clone();
        let menu = Popover::new("doc-menu")
            .menu()
            .min_w(px(MENU_W))
            .child(menu_w())
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if sub_bounds.get().is_none_or(|b| !b.contains(&ev.position)) {
                    w.close_doc_menu(cx);
                }
            }))
            // How this document is seen first: 页宽, then the 阅读模式, 显示引用列表 and 窗口保持在最前
            // switches (the user's order, 2026-11-25), then what is done to it.
            .when(!board, |d| d.child(
                menu_item("dm-width", "page-width", crate::settings::text("页宽设置"), true, sub == Some(DocSub::Width))
                    .child(record_row("dm-width"))
                    .on_hover(hover_sub(Some(DocSub::Width))),
            ))
            // (a board file has no reading mode or references)
            .when(!board_file, |d| d.child(
                // switch rows keep the menu open (`MenuItem::switch`): the click flips, nothing else
                menu_item("dm-read", "eye-open", crate::settings::text("阅读模式"), false, false)
                    .switch(reading)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| w.set_mode(!reading, window, cx))),
            )
            .child(
                menu_item("dm-refs", "list", crate::settings::text("显示引用列表"), false, false)
                    .switch(refs_shown)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_refs(!refs_shown, cx))),
            ))
            .when(crate::on_top::SUPPORTED, |d| d.child(
                menu_item("dm-on-top", crate::on_top::ICON, crate::settings::tr("窗口保持在最前", "Keep Window on Top"), false, false)
                    .switch(on_top)
                    .tip_right(crate::settings::tr("窗口会一直显示在其他应用的窗口之上", "Stays above other apps' windows"), None)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| w.set_on_top(!on_top, window, cx))),
            ))
            .when(!board || !board_file || crate::on_top::SUPPORTED, |d| d.child(divider()))
            .when(!board, |d| d.child(
                menu_item("dm-find", "text-search", crate::settings::text("查找和替换"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(|w, _, window, cx| w.find_in_doc(false, window, cx))),
            )
            .child(divider()))
            .child(
                menu_item("dm-move", "folder-input", crate::settings::text("移动到"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| {
                        w.close_doc_menu(cx);
                        w.open_move(vec![mv_id.clone()], window, cx);
                    })),
            )
            .child(
                menu_item(
                    "dm-pin",
                    if pinned { "pin-off" } else { "pin" },
                    if pinned { crate::settings::text("从“置顶”移除") } else { crate::settings::text("添加到“置顶”") },
                    false,
                    false,
                )
                .on_hover(hover_sub(None))
                .on_click(cx.listener(move |w, _, _, cx| {
                    w.toggle_pin(&pin_id, cx);
                    w.close_doc_menu(cx);
                })),
            )
            .child(
                menu_item("dm-star", if starred { "star-off" } else { "star" }, if starred { crate::settings::text("取消收藏") } else { crate::settings::text("添加到收藏") }, false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_star(&star_id, cx))),
            )
            .child(divider())
            .when(child, |d| d.child(
                menu_item("dm-child", "plus", crate::settings::tr("新建子文档", "New child page"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| { w.close_doc_menu(cx); w.new_child(&child_id, window, cx); })),
            ))
            .child(
                menu_item("dm-copy", "copy", crate::settings::text("创建副本"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| w.duplicate_doc(&dup_id, window, cx))),
            )
            .child(
                menu_item("dm-download", "export", crate::settings::text("下载为"), true, sub == Some(DocSub::Download))
                    .child(record_row("dm-download"))
                    .on_hover(hover_sub(Some(DocSub::Download))),
            )
            .when(!board_file, |d| d.child(
                menu_item("dm-copy-md", "doc-md", crate::settings::text("复制为 Markdown"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(|w, _, _, cx| {
                        w.close_doc_menu(cx);
                        if let Some(ed) = w.editor.clone() {
                            ed.update(cx, |e, cx| e.copy_markdown(cx));
                            w.show_toast(crate::settings::text("已复制为 Markdown"), cx);
                        }
                    })),
            ))
            .child(divider())
            .when(crate::printing::SUPPORTED, |d| d.child(
                menu_item("dm-print", "printer", crate::settings::text("打印"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, window, cx| w.print_doc(&print_id, window, cx))),
            ).child(divider()))
            .child(
                menu_item("dm-info", "info", crate::settings::text("文档信息"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(|w, _, window, cx| w.open_doc_info(window, cx))),
            )
            .child(
                menu_item("dm-history", "restore", crate::settings::tr("版本历史", "Version history"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(|w, _, window, cx| {
                        w.close_doc_menu(cx);
                        if let Some(id) = w.current.clone() { w.open_history(&id, window, cx) }
                    })),
            )
            .child(divider())
            .child(
                menu_item("dm-delete", "bin", crate::settings::text("删除"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&del_id, cx))),
            )
            // (a board has its own shortcut help, `?`)
            .when(!board, |d| d.child(divider())
            .child(
                menu_item("dm-shortcuts", "keyboard", crate::settings::text("快捷键"), false, false)
                    .on_hover(hover_sub(None))
                    .on_click(cx.listener(|w, _, _, cx| {
                        w.close_doc_menu(cx);
                        w.shortcuts = true;
                        cx.notify();
                    })),
            ));
        // hung from the measured button; not yet measured: not drawn yet
        let Some(b) = self.more_btn_bounds.get() else { return vec![] };
        let (at, _) = crate::design::hang(b, crate::design::Align::End, None);
        let (right, menu_top) = (at.x, at.y);
        let mut out = vec![
            deferred(anchored().anchor(Anchor::TopRight).position(point(right, menu_top)).child(menu.fit(crate::design::room(point(right, menu_top), Anchor::TopRight))))
                .with_priority(2)
                .into_any_element(),
        ];
        // Submenus open to the left, 4px from the menu, level with their item.
        let sub_at = point(right - px(self.doc_menu_w.get().unwrap_or(MENU_W) + 4.), px(0.));
        let record = self.doc_sub_bounds.clone();
        record.set(None);
        let recorder = move || {
            let record = record.clone();
            canvas(move |b, _, _| record.set(Some(b)), |_, _, _, _| {}).absolute().inset_0()
        };
        let sub_el = match sub {
            // each levels its first item with its row, where the row was last painted
            Some(DocSub::Width) => row_top("dm-width").map(|top| (top - PAD, self.width_panel(width, cx).child(recorder()).into_any_element())),
            // Level the submenu's first item with 下载为.
            Some(DocSub::Download) => row_top("dm-download").map(|top| (
                top - PAD,
                {
                    let pdf = id.clone();
                    Popover::new("dm-download-menu")
                        .menu()
                        .min_w(px(130.))
                        .child(recorder())
                        .when(crate::printing::SUPPORTED, |d| d.child(sub_item("dm-pdf", "PDF").on_click(cx.listener(move |w, _, window, cx| w.export_pdf(&pdf, window, cx)))))
                        .child(sub_item("dm-md", "Markdown").on_click(cx.listener(move |w, _, _, cx| w.open_export_md(&id, cx))))
                        .into_any_element()
                },
            )),
            None => None,
        };
        if let Some((top, el)) = sub_el {
            out.push(
                deferred(anchored().anchor(Anchor::TopRight).position(point(sub_at.x, px(top))).snap_to_window_with_margin(px(8.)).child(el))
                    .with_priority(2)
                    .into_any_element(),
            );
        }
        out
    }

    /// 为当前窗口选择合适页宽: 默认 / 较宽 / 全宽 cards. The panel takes its width from its
    /// contents (the caption, or three cards each as wide as its longest label) so no
    /// language wraps; the cards share it equally.
    fn width_panel(&self, current: PageWidth, cx: &mut Context<Self>) -> Popover {
        use crate::design::{color, geo};
        // A choice card: the focus ring round the picked one, `accent` under the pointer.
        let card = |id: &'static str, label: &'static str, w: PageWidth, line: f32| {
            let on = w == current;
            let line_color = if on { color::foreground() } else { color::muted_foreground() };
            div()
                .id(id)
                .flex_1()
                .min_w(px(64.))
                .h(px(66.))
                .px(px(8.))
                .pt(px(4.))
                .flex()
                .flex_col()
                .items_center()
                .rounded(px(geo().button_r))
                .border_1()
                // the picked card wears the focus ring (shadcn: a `ring` edge and a 3pt `ring/50`
                // halo), as a field with the keyboard in it does; not a thin near-black line
                .border_color(if on { color::ring() } else { color::border() })
                .when(on, |d| d.shadow(vec![crate::design::shadow::ring(color::focus_ring(), 3.)]))
                .bg(color::popover())
                .cursor_pointer()
                .when(!on, |d| d.hover(|s| s.bg(color::accent())))
                .child(
                    div()
                        .w(px(46.))
                        .h(px(28.))
                        .pt(px(7.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .rounded(px(2.))
                        .bg(color::muted())
                        .children((0..4).map(move |_| div().w(px(line)).h(px(2.)).flex_none().bg(line_color))),
                )
                .child(
                    div()
                        .mt(px(6.))
                        .whitespace_nowrap()
                        .text_size(px(geo().text_xs))
                        .line_height(px(18.))
                        .text_color(if on { color::foreground() } else { color::muted_foreground() })
                        .child(crate::settings::label(label)),
                )
                .on_click(cx.listener(move |ws, _, _, cx| ws.set_page_width(w, cx)))
        };
        Popover::new("dm-width-panel")
            .relative()
            .min_w(px(238.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(px(geo().text_xs))
                    .line_height(px(18.))
                    .text_color(color::muted_foreground())
                    .child(crate::settings::text("为当前窗口选择合适页宽")),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .gap(px(12.))
                    .child(card("pw-standard", crate::settings::text("默认"), PageWidth::Standard, 20.))
                    .child(card("pw-wide", crate::settings::text("较宽"), PageWidth::Wide, 28.))
                    .child(card("pw-full", crate::settings::text("全宽"), PageWidth::Full, 38.)),
            )
    }
}
