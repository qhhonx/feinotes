//! 我的文档 as a page tree: parent links kept in the
//! index, the sidebar's expand state, 新建子页面, 移动到 and its dialog, and
//! the ancestor chain used by the header breadcrumbs.

use crate::app::Workspace;
use crate::settings::display_title;
use crate::editor::view::SubPage;
use crate::input::{TextField, TextFieldEvent};
use crate::store::Entry;
use crate::theme::{self, Theme};
use crate::design::{Button, Dialog, Field, FieldSize, Separator, scrim};
use crate::ui::{ROW_RADIUS, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::collections::{HashMap, HashSet};

/// Children of every page, keyed by parent (`None`: the top level, in list
/// order); siblings oldest first. A page whose parent is gone, or that sits
/// in a parent cycle, is shown at the top level.
pub(crate) struct Tree<'a> {
    roots: Vec<&'a Entry>,
    kids: HashMap<&'a str, Vec<&'a Entry>>,
    /// Folder → its 正文 (#357), shown as the folder's own row, not among its children.
    notes: HashMap<&'a str, &'a str>,
}

impl<'a> Tree<'a> {
    pub fn new(entries: &'a [Entry]) -> Self {
        Self::with_order(entries, None)
    }

    /// 我的文档 ordered by the page list's setting (`order`), or — when it is
    /// not given — by the arrangement the user dragged into place.
    pub fn with_order(entries: &'a [Entry], order: Option<crate::home::Order>) -> Self {
        // Only folders take children. Every step over all entries is one lookup in
        // this small table, not a set of every id: the tree is rebuilt each frame
        // the sidebar draws, and a vault of 15,000 notes made that 5 ms a frame.
        let mut fids: Vec<&str> = vec![];
        let mut folder: rustc_hash::FxHashMap<&str, usize> = Default::default();
        for e in entries.iter().filter(|e| e.meta.folder.is_some()) {
            folder.entry(e.id.as_str()).or_insert_with(|| { fids.push(e.id.as_str()); fids.len() - 1 });
        }
        let parent = |e: &'a Entry| e.meta.parent.as_deref().filter(|p| *p != e.id).and_then(|p| folder.get(p).copied());
        let notes: HashMap<&str, &str> = entries.iter().filter_map(|e| Some((e.id.as_str(), e.note.as_deref()?))).collect();
        let is_note: HashSet<&str> = notes.values().copied().collect();
        // 本机文件 are not in 我的文档: the sidebar lists them on their own.
        let shown = |e: &&'a Entry| !e.meta.local() && (is_note.is_empty() || !is_note.contains(e.id.as_str()));
        let mut roots: Vec<&Entry> = vec![];
        let mut kids: Vec<Vec<&Entry>> = vec![vec![]; fids.len()];
        let mut lifted: Vec<(&Entry, usize)> = vec![];
        for e in entries.iter().filter(shown) {
            match parent(e) {
                Some(p) => { kids[p].push(e); lifted.push((e, p)); }
                None => roots.push(e),
            }
        }
        // Lift pages unreachable from the top level (cycles), in list order.
        let mut seen = vec![false; fids.len()];
        let mut stack: Vec<usize> = roots.iter().filter_map(|e| folder.get(e.id.as_str()).copied()).collect();
        while let Some(f) = stack.pop() {
            if !std::mem::replace(&mut seen[f], true) {
                stack.extend(kids[f].iter().filter_map(|e| folder.get(e.id.as_str()).copied()));
            }
        }
        roots.extend(lifted.into_iter().filter(|(_, p)| !seen[*p]).map(|(e, _)| e));
        // `Meta::place`, then the id: the list is re-read often (each time the
        // window comes back) and must come out the same every time.
        let place = |a: &Entry, b: &Entry| (a.meta.place(), &a.id).cmp(&(b.meta.place(), &b.id));
        // `sort_by` hands out `&&Entry`; the comparisons take `&Entry`.
        let cmp = |a: &&Entry, b: &&Entry| match order {
            Some(o) => o.cmp(a, b),
            None => place(a, b),
        };
        roots.sort_by(cmp);
        let kids = kids.into_iter().enumerate().filter(|(f, v)| seen[*f] && !v.is_empty())
            .map(|(f, mut v)| { v.sort_by(cmp); (fids[f], v) }).collect();
        Tree { roots, kids, notes }
    }

    pub fn children(&self, id: Option<&str>) -> &[&'a Entry] {
        match id {
            None => &self.roots,
            Some(id) => self.kids.get(id).map_or(&[], |v| v.as_slice()),
        }
    }

    /// Pre-order rows with their depth, descending into pages `open` says yes to.
    pub fn rows(&self, open: impl Fn(&str) -> bool) -> Vec<(&'a Entry, usize)> {
        let mut out = vec![];
        let mut stack: Vec<(&Entry, usize)> = self.children(None).iter().rev().map(|e| (*e, 0)).collect();
        while let Some((e, d)) = stack.pop() {
            out.push((e, d));
            if open(&e.id) {
                stack.extend(self.children(Some(&e.id)).iter().rev().map(|c| (*c, d + 1)));
            }
        }
        out
    }

    /// `id` and every page under it, folders' 正文 included.
    pub fn subtree(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_string()];
        let mut i = 0;
        while i < out.len() {
            let mut kids: Vec<String> = self.notes.get(out[i].as_str()).map(|n| n.to_string()).into_iter().collect();
            kids.extend(self.children(Some(&out[i])).iter().map(|e| e.id.clone()));
            out.extend(kids);
            i += 1;
        }
        out
    }
}

/// What a sidebar row or a Home card shows of `e`: its links, reminders and body stay behind.
pub(crate) fn face(e: &Entry) -> Entry {
    Entry { id: e.id.clone(), meta: e.meta.clone(), title: e.title.clone(), icon: e.icon.clone(), body: Default::default(),
        reminders: vec![], links: vec![], note: e.note.clone(), top: e.top.clone() }
}

/// The 移动到 dialog.
pub struct MoveDialog {
    /// Pages being moved (each with its subtree).
    pub ids: Vec<String>,
    /// Chosen destination: `Some(None)` is the top level of 我的文档.
    pub target: Option<Option<String>>,
    pub expanded: HashSet<String>,
    pub search: Entity<TextField>,
    pub scroll: ScrollHandle,
    _subs: Vec<Subscription>,
}

impl Workspace {
    /// Ancestors of `id`, top level first.
    pub(crate) fn ancestors(&self, id: &str) -> Vec<&Entry> {
        let mut out: Vec<&Entry> = vec![];
        let mut cur = self.entries.iter().find(|e| e.id == id);
        while let Some(p) = cur.and_then(|e| e.meta.parent.as_deref()).and_then(|p| self.entries.iter().find(|e| e.id == p)) {
            if p.id == id || out.iter().any(|e| e.id == p.id) {
                break;
            }
            out.push(p);
            cur = Some(p);
        }
        // A folder's 正文 is the folder's row: the folder is not above it.
        if out.first().is_some_and(|f| f.note.as_deref() == Some(id)) { out.remove(0); }
        out.reverse();
        out
    }

    /// The row document `id` is shown as: its folder when it is the folder's
    /// 正文 (#357), so 删除, 移动 and 重命名 take the whole folder.
    pub(crate) fn row_of(&self, id: &str) -> String {
        self.entries.iter().find(|e| e.note.as_deref() == Some(id)).map_or_else(|| id.to_string(), |f| f.id.clone())
    }

    /// The document row `id` opens: a folder's 正文, or itself.
    pub(crate) fn note_or(&self, id: &str) -> String {
        self.entries.iter().find(|e| e.id == id).and_then(|e| e.note.clone()).unwrap_or_else(|| id.to_string())
    }

    /// Whether 新建子文档 is offered on `id`'s row: a document of the library, or one with children.
    pub(crate) fn takes_children(&self, id: &str) -> bool {
        self.entries.iter().find(|e| e.id == id).is_some_and(|e| match e.meta.folder {
            Some(_) => e.note.is_some(),
            None => !e.meta.local() && e.meta.kind.is_doc() && !e.meta.markdown && self.store.can_have_children(id),
        })
    }

    /// The folder rows show their 正文's title and icon, also while it is typed.
    pub(crate) fn sync_note_rows(&mut self) {
        let notes: HashMap<String, usize> = self.entries.iter().enumerate().filter(|(_, e)| e.note.is_some()).map(|(i, e)| (e.note.clone().unwrap(), i)).collect();
        if notes.is_empty() { return; }
        let picks: Vec<(usize, String, Option<String>)> = self.entries.iter().filter_map(|e| notes.get(&e.id).map(|&f| (f, e.title.clone(), e.icon.clone()))).collect();
        // Only when one differs: this runs on every list, and a change rebuilds the rows.
        if picks.iter().all(|(f, title, icon)| self.entries[*f].title == *title && self.entries[*f].icon == *icon) { return; }
        let entries = self.entries.edit();
        for (f, title, icon) in picks { let e = &mut entries[f]; e.title = title; e.icon = icon; }
    }

    /// Expand the sidebar down to `id`.
    pub(crate) fn reveal_in_tree(&mut self, id: &str) {
        let ids: Vec<String> = self.ancestors(id).iter().map(|e| e.id.clone()).collect();
        self.expanded.extend(ids);
    }

    /// Hand the open document its child pages (for 子页面列表 blocks).
    pub(crate) fn sync_sub_pages(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else { return };
        let pages = Tree::new(&self.entries)
            .children(self.entries.iter().find(|e| e.id == id).and_then(|e| e.meta.parent.as_deref()))
            .iter()
            .filter(|e| e.id != id)
            .map(|e| SubPage { id: e.id.clone(), title: e.title.clone(), icon: e.icon.clone() })
            .collect();
        ed.update(cx, |e, cx| e.set_sub_pages(pages, cx));
        self.sync_links(cx);
    }

    /// A new page under `parent`, opened: in a folder (the folder's "+" →
    /// 文档), or 新建子文档 under a document or a document with children (#357),
    /// which puts a document without children in a folder of its name first.
    pub fn new_child(&mut self, parent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.entries.iter().find(|e| e.id == parent) else { return };
        let plain_folder = e.meta.folder.is_some() && e.note.is_none();
        if !plain_folder && !self.takes_children(parent) { return; }
        let p = parent.to_string();
        if !self.save_before(cx, move |w, window, cx| w.new_child(&p, window, cx)) {
            return;
        }
        let file_before = self.store.track_file_changes();
        let made = if plain_folder { self.store.create().and_then(|(id, _)| self.store.set_parent(&id, Some(parent)).map(|_| id)) }
            else { self.store.create_child(parent, &crate::model::Document::default()) };
        match made {
            Ok(id) => {
                self.entries.set(self.store.list());
                self.sort();
                if let Some(folder) = self.entries.iter().find(|e| e.id == id).and_then(|e| e.meta.parent.clone()) {
                    self.expanded.insert(folder.clone());
                    self.reveal_in_tree(&folder);
                }
                self.open(&id, window, cx);
                if let Some(ed) = &self.editor { let f = ed.read(cx).focus.clone(); window.focus(&f, cx); }
            }
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.remember_files(file_before, if plain_folder { crate::settings::tr("新建文档", "New document") } else { crate::settings::tr("新建子文档", "New subdocument") });
        cx.notify();
    }

    pub(crate) fn open_move(&mut self, ids: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let ids: Vec<String> = ids.iter().map(|id| self.row_of(id)).collect();
        let search = cx.new(|cx| {
            let mut f = TextField::new("搜索目标位置", cx);
            f.placeholder_color = Theme::text_placeholder();
            f
        });
        let subs = vec![cx.subscribe_in(&search, window, |w, _, ev: &TextFieldEvent, window, cx| {
            if let TextFieldEvent::Cancel = ev {
                w.close_move(window, cx);
            }
            cx.notify();
        })];
        window.focus(&search.focus_handle(cx), cx);
        self.move_dlg = Some(MoveDialog { ids, target: None, expanded: HashSet::new(), search, scroll: ScrollHandle::new(), _subs: subs });
        cx.notify();
    }

    fn close_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.move_dlg = None;
        if let Some(ed) = &self.editor {
            { let f = ed.read(cx).focus.clone(); window.focus(&f, cx); }
        }
        cx.notify();
    }

    /// 移动到: hang each page under `target` (`None`: top level), skipping
    /// moves into its own subtree.
    /// Shown at once and written off the window's thread, as a drop in the
    /// sidebar is (`tree_writes.rs`): moving a folder rewrites the links across
    /// its edge. One that fails goes back where it was, with a toast saying why.
    pub(crate) fn move_docs(&mut self, ids: &[String], target: Option<String>, cx: &mut Context<Self>) {
        use crate::tree_writes::{PendingMove, TreeWrite};
        let ids: Vec<String> = ids.iter().map(|id| self.row_of(id)).collect();
        let ids: Vec<String> = ids.into_iter().filter(|id| !target.as_ref().is_some_and(|t| Tree::new(&self.entries).subtree(id).contains(t))).collect();
        if ids.is_empty() { return; }
        let file_before = self.entries.to_vec();
        let mut moves: Vec<PendingMove> = ids.iter().map(|id| PendingMove { id: id.clone(), parent: target.clone(), before: None, history: 0, joined: ids.len() > 1 }).collect();
        for m in &moves { self.apply_tree_move(m); }
        // One step of the file history for all of them.
        let history = self.remember_tree_move(file_before, crate::settings::tr("移动", "Move"));
        for mut m in moves.drain(..) { m.history = history; self.queue_tree_write(TreeWrite::Move(m), cx); }
        if let Some(t) = &target {
            self.expanded.insert(t.clone());
            self.reveal_in_tree(&t.clone());
        }
        self.sync_sub_pages(cx);
        self.sort();
        self.show_toast(crate::settings::text("移动成功"), cx);
        cx.notify();
    }

    /// The 移动到 dialog: 我的文档 on the left, its page tree on the right.
    /// Quiet selection fills, a 30pt search field, the modal buttons.
    pub(crate) fn render_move_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dlg = self.move_dlg.as_ref()?;
        let tree = Tree::new(&self.entries);
        let hidden: HashSet<String> = dlg.ids.iter().flat_map(|id| tree.subtree(id)).collect();
        let q = dlg.search.read(cx).text.trim().to_lowercase();
        let border = crate::design::color::border();

        let search = Field::new("move-search")
            .sized(FieldSize::Sm)
            .mt(px(12.))
            .mx(px(8.))
            .mb(px(10.))
            .child(crate::design::icon("search", 16., crate::design::color::muted_foreground()))
            .child(div().flex_1().min_w_0().flex().child(dlg.search.clone()));
        // The location column reads like a sidebar: a grey surface and
        // a quiet pill for the selected location.
        let left = div()
            .w(px(240.))
            .flex_none()
            .flex()
            .flex_col()
            .bg(Theme::bg_overlay())
            .rounded_l(px(9.))
            .child(search)
            .child(
                div()
                    .mx(px(8.))
                    .h(px(30.))
                    .pl(px(10.))
                    .pr(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(9.))
                    .bg(Theme::fill_hover_strong())
                    .child(icon("library", 15., Theme::icon()))
                    .child(div().ml(px(8.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(self.home_name())),
            );

        let row = |key: String, selected: bool| {
            div()
                .id(SharedString::from(key))
                .h(px(30.))
                .mb(px(2.))
                .pr(px(5.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(selected, |d| d.bg(Theme::fill_hover_strong()))
                .when(!selected, |d| d.hover(|s| s.bg(Theme::fill_hover())))
        };
        let title = |t: String| div().ml(px(9.)).flex_1().min_w_0().text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(t));
        let page_icon = |e: &Entry| match &e.icon {
            _ if e.meta.folder.is_some() && e.note.is_none() => icon("folder", 16., Theme::icon()).into_any_element(),
            Some(em) => div().size(px(16.)).flex_none().flex().items_center().justify_center().text_size(px(14.)).child(em.clone()).into_any_element(),
            None => icon(crate::search::file_glyph(&e.meta), 16., Theme::icon()).into_any_element(),
        };
        let mut list = div().id("move-tree").flex_1().min_w_0().px(px(8.)).py(px(12.)).overflow_y_scroll().track_scroll(&dlg.scroll);
        if q.is_empty() {
            list = list.child(
                row("move-root".into(), dlg.target == Some(None))
                    .pl(px(16.))
                    .child(icon("library", 16., Theme::icon()))
                    .child(title(crate::settings::text("我的文档").into()))
                    .on_click(cx.listener(|w, _, _, cx| {
                        if let Some(d) = w.move_dlg.as_mut() {
                            d.target = Some(None);
                        }
                        cx.notify();
                    })),
            );
            for (e, depth) in tree.rows(|id| dlg.expanded.contains(id)) {
                if hidden.contains(&e.id) || e.meta.folder.is_none() {
                    continue;
                }
                let kids = tree.children(Some(&e.id)).iter().any(|c| !hidden.contains(&c.id));
                let open = dlg.expanded.contains(&e.id);
                let (pick, toggle) = (e.id.clone(), e.id.clone());
                list = list.child(
                    row(format!("move-{}", e.id), dlg.target.as_ref().is_some_and(|t| t.as_deref() == Some(e.id.as_str())))
                        .pl(px(16. + 20. * depth as f32))
                        .child(
                            div()
                                .id(SharedString::from(format!("move-arrow-{}", e.id)))
                                .size(px(20.))
                                .p(px(5.))
                                .mr(px(8.))
                                .flex_none()
                                .rounded(px(5.))
                                .when(kids, |d| {
                                    d.hover(|s| s.bg(Theme::fill_hover_strong()))
                                        .child(icon("tree-triangle", 10., Theme::icon()).when(open, |s| s.with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)))))
                                        .on_click(cx.listener(move |w, _, _, cx| {
                                            cx.stop_propagation();
                                            if let Some(d) = w.move_dlg.as_mut()
                                                && !d.expanded.remove(&toggle) {
                                                    d.expanded.insert(toggle.clone());
                                                }
                                            cx.notify();
                                        }))
                                }),
                        )
                        .child(page_icon(e))
                        .child(title(display_title(&e.title)))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            if let Some(d) = w.move_dlg.as_mut() {
                                d.target = Some(Some(pick.clone()));
                            }
                            cx.notify();
                        })),
                );
            }
        } else {
            let hits: Vec<&Entry> = self.entries.iter().filter(|e| e.meta.folder.is_some() && !hidden.contains(&e.id) && display_title(&e.title).to_lowercase().contains(&q)).collect();
            if hits.is_empty() {
                list = list.child(div().pt(px(80.)).flex().justify_center().text_size(px(theme::ui_font())).text_color(Theme::text_caption()).child(crate::settings::text("无搜索结果")));
            }
            for e in hits {
                let pick = e.id.clone();
                list = list.child(
                    row(format!("move-hit-{}", e.id), dlg.target.as_ref().is_some_and(|t| t.as_deref() == Some(e.id.as_str())))
                        .pl(px(16.))
                        .child(page_icon(e))
                        .child(title(display_title(&e.title)))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            if let Some(d) = w.move_dlg.as_mut() {
                                d.target = Some(Some(pick.clone()));
                            }
                            cx.notify();
                        })),
                );
            }
        }

        let ready = dlg.target.is_some();
        // 确定 waits for a target.
        let ok = Button::primary("move-ok").label(crate::settings::text("确定")).disabled(!ready);
        let dialog = Dialog::new("move-dialog")
            .relative()
            .w(px(828.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .title(crate::settings::text("从 我的文档 移动到"))
            .closable(cx.listener(|w, _, window, cx| w.close_move(window, cx)))
            .child(
                div()
                    .h(px(512.))
                    .flex()
                    .border_1()
                    .border_color(border)
                    .rounded(px(crate::design::geo().button_r))
                    .overflow_hidden()
                    .child(left)
                    .child(Separator::new().vertical())
                    .child(list),
            )
            .footer(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(Button::outline("move-cancel").label(crate::settings::text("取消")).on_click(cx.listener(|w, _, window, cx| w.close_move(window, cx))))
                    .child(ok.on_click(cx.listener(|w, _, window, cx| {
                        let Some(d) = w.move_dlg.as_ref() else { return };
                        let Some(target) = d.target.clone() else { return };
                        let ids = d.ids.clone();
                        w.batch = None;
                        w.move_docs(&ids, target, cx);
                        w.close_move(window, cx);
                    }))),
            );
        Some(
            deferred(
                scrim()
                    .id("move-mask")
                    .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| w.close_move(window, cx)))
                    .child(dialog),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Tree;
    use std::collections::{HashMap, HashSet};
    use crate::store::Entry;
    use crate::home::{HomeSort, Order, SortDir};
    use crate::store::Meta;

    // The tree as it was built before #big-library-perf, to hold the faster one to.
    impl<'a> Tree<'a> {
    fn old(entries: &'a [Entry], order: Option<crate::home::Order>) -> Self {
        let ids: HashSet<&str> = entries.iter().filter(|e| e.meta.folder.is_some()).map(|e| e.id.as_str()).collect();
        let parent = |e: &'a Entry| e.meta.parent.as_deref().filter(|p| ids.contains(p) && *p != e.id);
        let notes: HashMap<&str, &str> = entries.iter().filter_map(|e| Some((e.id.as_str(), e.note.as_deref()?))).collect();
        let is_note: HashSet<&str> = notes.values().copied().collect();
        let mut roots: Vec<&Entry> = vec![];
        let mut kids: HashMap<&str, Vec<&Entry>> = HashMap::new();
        // 本机文件 are not in 我的文档: the sidebar lists them on their own.
        for e in entries.iter().filter(|e| !e.meta.local() && !is_note.contains(e.id.as_str())) {
            match parent(e) {
                Some(p) => kids.entry(p).or_default().push(e),
                None => roots.push(e),
            }
        }
        // Lift pages unreachable from the top level (cycles).
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = roots.iter().map(|e| e.id.as_str()).collect();
        while let Some(id) = stack.pop() {
            if seen.insert(id) {
                stack.extend(kids.get(id).map_or(vec![], |v| v.iter().map(|e| e.id.as_str()).collect()));
            }
        }
        for e in entries.iter().filter(|e| !e.meta.local() && !is_note.contains(e.id.as_str())) {
            if !seen.contains(e.id.as_str()) {
                seen.insert(&e.id);
                if let Some(v) = parent(e).and_then(|p| kids.get_mut(p)) {
                    v.retain(|x| x.id != e.id);
                }
                roots.push(e);
            }
        }
        // `Meta::place`, then the id: the list is re-read often (each time the
        // window comes back) and must come out the same every time.
        let place = |a: &Entry, b: &Entry| (a.meta.place(), &a.id).cmp(&(b.meta.place(), &b.id));
        // `sort_by` hands out `&&Entry`; the comparisons take `&Entry`.
        let cmp = |a: &&Entry, b: &&Entry| match order {
            Some(o) => o.cmp(a, b),
            None => place(a, b),
        };
        roots.sort_by(cmp);
        for v in kids.values_mut() {
            v.sort_by(cmp);
        }
        Tree { roots, kids, notes }
    }
    }

    fn entry(id: String, folder: bool, parent: Option<String>, place: u64) -> Entry {
        let meta = Meta { folder: folder.then(|| id.clone()), parent, top_order: Some(place % 7), modified: place, created: place / 3, ..Meta::default() };
        Entry { id, meta, title: format!("t{}", place % 5), icon: None, body: "".into(), reminders: vec![], links: vec![], note: None, top: String::new() }
    }

    fn shape<'a>(t: &Tree<'a>, entries: &[Entry]) -> (Vec<(String, usize)>, Vec<(String, Vec<String>)>) {
        let rows = t.rows(|_| true).into_iter().map(|(e, d)| (e.id.clone(), d)).collect();
        let kids = entries.iter().map(|e| (e.id.clone(), t.children(Some(&e.id)).iter().map(|c| c.id.clone()).collect())).collect();
        (rows, kids)
    }

    /// Same rows, depths and children as the old build, on random libraries
    /// with cycles, pages that are their own parent, 本机文件 and folders' 正文.
    #[test]
    fn same_tree_as_before() {
        let mut seed = 7u64;
        let mut next = |n: u64| { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (seed >> 33) % n };
        let orders = [None, Some(Order { field: HomeSort::Title, dir: SortDir::Asc, folders_first: true }),
            Some(Order { field: HomeSort::Modified, dir: SortDir::Desc, folders_first: false })];
        for round in 0..300 {
            let n = 1 + next(60) as usize;
            let mut entries: Vec<Entry> = (0..n).map(|i| {
                let parent = match next(5) { 0 | 1 => None, 2 => Some(format!("e{}", next(n as u64 + 3))), _ => Some(format!("e{}", next(n as u64))) };
                entry(format!("e{i}"), next(3) == 0, parent, next(40))
            }).collect();
            for _ in 0..next(3) { let i = next(n as u64) as usize; entries[i].meta.path = Some("/x".into()); }
            for _ in 0..next(3) {
                let (f, d) = (next(n as u64) as usize, next(n as u64) as usize);
                if entries[f].meta.folder.is_some() && f != d { entries[f].note = Some(entries[d].id.clone()); }
            }
            for order in orders {
                let (a, b) = (Tree::with_order(&entries, order), Tree::old(&entries, order));
                assert_eq!(shape(&a, &entries), shape(&b, &entries), "round {round}");
            }
        }
    }
}
