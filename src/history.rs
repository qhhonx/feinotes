//! 版本历史: a drawer on the right while the page itself shows the chosen
//! snapshot in place, read-only — the Google Docs / 飞书 shape. The drawer is
//! the index (grouped by day, with the size of each change); the page is the
//! preview; 显示改动 tints the blocks that version added or rewrote.
//!
//! Nothing here writes the previewed text back: the editor is put in reading
//! mode (`Editor::set_preview`) and `flush` refuses while a preview is up.

use crate::app::{Workspace, HEADER_H};
use crate::editor::core::Core;
use crate::input::{TextField, TextFieldEvent};
use crate::theme::{self, Theme};
use crate::ui::{format_date, format_time};
use crate::design::{Button, ButtonSize, Popover, Segment, Segmented, menu as dmenu};
use fdl_core::changes::Change;
use fdl_core::model::BlockId;
use fdl_core::versions::Version;
use gpui::{prelude::FluentBuilder, *};
use std::collections::HashMap;

/// The drawer sits inside the note, flush with its right edge.
pub const DRAWER_W: f32 = 300.;
pub const BANNER_H: f32 = 40.;

pub struct HistoryView {
    pub id: String,
    pub versions: Vec<Version>,
    pub selected: Option<String>,
    /// 显示改动: tint the blocks this version added or rewrote.
    pub marks: bool,
    /// 仅看命名版本.
    pub named_only: bool,
    /// `+lines −lines` per snapshot, against the one before it.
    pub delta: HashMap<String, (usize, usize)>,
    pub error: Option<String>,
    pub list_scroll: ScrollHandle,
}

/// The editor state before the preview began — document, caret, selection and
/// undo stack — so 返回当前 is exact.
pub struct Preview {
    pub core: Core,
    pub reading: bool,
    pub name: String,
}

pub struct VersionRename {
    pub name: String,
    pub field: Entity<TextField>,
    _subs: Vec<Subscription>,
}

fn day_key(secs: u64) -> (i64, u32, u32) {
    let (y, m, d, _, _) = crate::time::civil(secs);
    (y, m, d)
}

fn day_label(secs: u64) -> SharedString {
    let now = crate::time::now();
    let day = day_key(secs);
    if day == day_key(now) {
        crate::settings::tr("今天", "Today").into()
    } else if day == day_key(now.saturating_sub(86_400)) {
        crate::settings::tr("昨天", "Yesterday").into()
    } else {
        format_date(secs).into()
    }
}

fn hhmm(secs: u64) -> String {
    let (_, _, _, h, mi) = crate::time::civil(secs);
    format!("{h:02}:{mi:02}")
}

/// `+12 行 −3 行`, or the note that this is the earliest snapshot.
fn delta_text(name: &str, delta: &HashMap<String, (usize, usize)>) -> String {
    match delta.get(name).copied() {
        None => crate::settings::tr("最早的一份", "Earliest").to_string(),
        // Same line count, different text (a rewrite inside one line): the
        // version exists, it just cannot be summarised in lines.
        Some((0, 0)) => crate::settings::tr("与上一份同行数", "Same length").to_string(),
        Some((a, 0)) => crate::localized_format!("+{} 行", "+{} lines", a),
        Some((0, r)) => crate::localized_format!("−{} 行", "−{} lines", r),
        Some((a, r)) => crate::localized_format!("+{} 行 −{} 行", "+{} −{} lines", a, r),
    }
}

impl Workspace {
    pub fn open_history(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let versions = self.store.versions(id);
        let mut delta = HashMap::new();
        for (i, v) in versions.iter().enumerate() {
            let Some(older) = versions.get(i + 1) else { continue };
            if let Ok(lines) = self.store.version_diff(id, &v.name, Some(&older.name)) {
                delta.insert(v.name.clone(), fdl_core::diff::counts(&lines));
            }
        }
        let first = versions.first().map(|v| v.name.clone());
        self.history = Some(HistoryView {
            id: id.to_string(),
            versions,
            selected: None,
            marks: true,
            named_only: false,
            delta,
            error: None,
            list_scroll: ScrollHandle::new(),
        });
        self.doc_menu = false;
        self.doc_sub = None;
        self.mark_history_drawer(true, cx);
        match first {
            Some(name) => self.preview_version(&name, window, cx),
            None => {
                self.preview = None;
                window.focus(&self.focus, cx);
                cx.notify();
            }
        }
    }

    /// Show a snapshot in the page, read-only. The first call remembers what
    /// the editor held so 返回当前 can put it back untouched.
    pub fn preview_version(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let Some(h) = self.history.as_ref() else { return };
        let id = h.id.clone();
        let marks_on = h.marks;
        let Some(index) = h.versions.iter().position(|v| v.name == name) else { return };
        let previous = h.versions.get(index + 1).map(|v| v.name.clone());
        let doc = match self.store.version_document(&id, name) {
            Ok(d) => d,
            Err(e) => return self.set_history_error(e.to_string(), cx),
        };
        // Marks must come from *this* parse: block ids are minted per parse,
        // so a separately parsed copy would tint nothing.
        let marks: HashMap<BlockId, Change> = match previous.filter(|_| marks_on) {
            Some(prev) => self.store.version_document(&id, &prev).ok()
                .map(|older| fdl_core::changes::block_changes(&older, &doc))
                .unwrap_or_default().into_iter().collect(),
            None => HashMap::new(),
        };
        if self.preview.is_none()
            && let Some(ed) = &self.editor
        {
            let e = ed.read(cx);
            self.preview = Some(Preview { core: e.core.clone(), reading: e.read_only, name: name.to_string() });
        }
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| {
                if e.table_file { e.reset_tables(); }
                e.receive_document(&Core::new(doc), cx);
                e.set_read_only(true, cx);
                e.set_preview(true, cx);
                e.diff_marks = (!marks.is_empty()).then_some(marks);
                // A much older version is often shorter: start at its top
                // instead of leaving the view past the end.
                e.list.scroll_to(ListOffset { item_ix: 0, offset_in_item: px(0.) });
                cx.notify();
            });
        }
        if let Some(p) = self.preview.as_mut() { p.name = name.to_string() }
        if let Some(h) = self.history.as_mut() {
            h.selected = Some(name.to_string());
            h.error = None;
        }
        cx.notify();
    }

    /// Back to the live document: the exact editor state from before.
    pub fn exit_preview(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.preview.take() else { return };
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| {
                if e.table_file { e.reset_tables(); }
                e.receive_document(&p.core, cx);
                e.diff_marks = None;
                e.set_preview(false, cx);
                e.set_read_only(p.reading, cx);
                cx.notify();
            });
        }
        if let Some(h) = self.history.as_mut() { h.selected = None }
        cx.notify();
    }

    pub fn close_history(&mut self, cx: &mut Context<Self>) {
        self.exit_preview(cx);
        self.history = None;
        self.version_rename = None;
        self.mark_history_drawer(false, cx);
        cx.notify();
    }

    /// Tell the editor whether Esc belongs to the drawer.
    fn mark_history_drawer(&self, on: bool, cx: &mut Context<Self>) {
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| {
                e.history_drawer = on;
                cx.notify();
            });
        }
    }

    /// Flip 显示改动 without reloading the page: only the tints change.
    pub fn toggle_history_marks(&mut self, cx: &mut Context<Self>) {
        let Some(h) = self.history.as_mut() else { return };
        h.marks = !h.marks;
        let on = h.marks;
        let Some(name) = h.selected.clone() else { return };
        let (id, versions) = (h.id.clone(), h.versions.clone());
        let previous = versions.iter().position(|v| v.name == name).and_then(|i| versions.get(i + 1).map(|v| v.name.clone()));
        // The snapshot on screen is the one the ids must match, so compare
        // against the editor's own document rather than a fresh parse.
        let shown = self.editor.as_ref().map(|ed| ed.read(cx).core.doc.clone());
        let marks: HashMap<BlockId, Change> = match (previous.filter(|_| on), shown) {
            (Some(prev), Some(shown)) => self.store.version_document(&id, &prev).ok()
                .map(|older| fdl_core::changes::block_changes(&older, &shown))
                .unwrap_or_default().into_iter().collect(),
            _ => HashMap::new(),
        };
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| {
                e.diff_marks = (!marks.is_empty()).then_some(marks);
                cx.notify();
            });
        }
        cx.notify();
    }

    pub fn toggle_history_named(&mut self, cx: &mut Context<Self>) {
        if let Some(h) = self.history.as_mut() { h.named_only = !h.named_only }
        cx.notify();
    }

    /// 保存当前版本: bypasses the interval; identical text still adds nothing.
    pub fn save_version_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) { return }
        let Some(h) = self.history.as_ref() else { return };
        let id = h.id.clone();
        match self.store.save_version_now(&id) {
            Ok(true) => self.show_toast_kind(crate::editor_ui::ToastKind::Success, crate::settings::tr("已保存当前版本", "Saved a version"), cx),
            // Identical text never adds a snapshot; say so rather than let the
            // click look broken.
            Ok(false) => self.show_toast_kind(crate::editor_ui::ToastKind::Info, crate::settings::tr("内容与上一份相同，没有新增版本", "Same as the newest version; nothing added"), cx),
            Err(e) => return self.set_history_error(e.to_string(), cx),
        }
        self.refresh_history(window, cx);
    }

    /// Re-read the list, keeping the preview where it is (or on the newest).
    fn refresh_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.history.as_ref() else { return };
        let id = h.id.clone();
        let keep = h.selected.clone();
        let versions = self.store.versions(&id);
        let mut delta = HashMap::new();
        for (i, v) in versions.iter().enumerate() {
            let Some(older) = versions.get(i + 1) else { continue };
            if let Ok(lines) = self.store.version_diff(&id, &v.name, Some(&older.name)) {
                delta.insert(v.name.clone(), fdl_core::diff::counts(&lines));
            }
        }
        // Only follow a version if the page was already showing one: 保存当前
        // 版本 from the live document must leave the user on the live document.
        let was_previewing = self.preview.is_some();
        let target = keep
            .filter(|k| versions.iter().any(|v| &v.name == k))
            .or_else(|| was_previewing.then(|| versions.first().map(|v| v.name.clone())).flatten());
        if let Some(h) = self.history.as_mut() {
            h.versions = versions;
            h.delta = delta;
            h.error = None;
        }
        match target {
            Some(name) => self.preview_version(&name, window, cx),
            None => {
                // The last version went away: the page must not stay on it.
                if was_previewing { self.exit_preview(cx) }
                if let Some(h) = self.history.as_mut() { h.selected = None }
                cx.notify();
            }
        }
    }

    fn is_table(&self, id: &str) -> bool { self.entries.iter().any(|e| e.id == id && e.meta.kind == crate::store::DocKind::Table) }

    pub fn restore_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, name)) = self.history.as_ref().and_then(|h| h.selected.clone().map(|n| (h.id.clone(), n))) else { return };
        // a 数据表 writes as it is edited: nothing of the present may be pending when its file is replaced
        let table = self.is_table(&id);
        // a build without 数据表 leaves its file as it is
        if table && !crate::DATATABLE { return self.set_history_error(crate::settings::tr("此版本不支持数据表", "Data tables aren't supported in this version").to_string(), cx); }
        if table { if let Some(ed) = self.editor.clone() { ed.update(cx, |e, cx| e.commit_tables(cx)); } crate::table::flush_all(cx); }
        let doc = match self.store.restore_version(&id, &name) {
            Ok(d) => d,
            Err(e) => return self.set_history_error(e.to_string(), cx),
        };
        // Leave the preview first: from here on the editor holds the live
        // document again, and `flush` must work.
        self.exit_preview(cx);
        self.history = None;
        if self.store.save(&id, &doc).is_err() {
            self.show_toast_kind(crate::editor_ui::ToastKind::Error, crate::settings::tr("恢复失败：当前版本未被改动。", "Restore failed; the current version was left alone."), cx);
            return;
        }
        if table && let Some(p) = self.store.file_path(&id) { crate::table::reload(&p, cx); }
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| { if e.table_file { e.reset_tables(); } e.receive_document(&Core::new(doc), cx) });
        }
        self.unsaved = false;
        self.status = crate::app::SaveStatus::Saved;
        self.sync_document(window, cx);
        self.entries.set(self.store.list());
        self.sort();
        self.show_toast_kind(crate::editor_ui::ToastKind::Success, crate::settings::tr("已恢复到所选版本", "Restored that version"), cx);
    }

    pub fn version_as_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, name)) = self.history.as_ref().and_then(|h| h.selected.clone().map(|n| (h.id.clone(), n))) else { return };
        let meta = self.history.as_ref().and_then(|h| h.versions.iter().find(|v| v.name == name)).map(|v| (v.label.clone(), v.ms));
        if self.is_table(&id) {
            let (label, ms) = meta.unwrap_or_default();
            let tag = label.unwrap_or_else(|| match ms { 0 => String::new(), ms => hhmm(ms / 1000) });
            let base = self.entries.iter().find(|e| e.id == id).map(|e| crate::settings::display_title(&e.title).to_string()).unwrap_or_default();
            let title = if tag.is_empty() { base } else { crate::localized_format!("{}（{}）", "{} ({})", base, tag) };
            match self.store.copy_table_version(&id, &name, &title) {
                Ok(new) => {
                    self.entries.set(self.store.list());
                    self.sort();
                    self.close_history(cx);
                    self.open(&new, window, cx);
                }
                Err(e) => self.set_history_error(e.to_string(), cx),
            }
            return;
        }
        match self.store.version_document(&id, &name) {
            Ok(mut doc) => {
                // Name the copy after *which* snapshot it is: a bare "副本" or
                // "的历史版本" says nothing when several exist side by side.
                let (label, ms) = meta.unwrap_or_default();
                let tag = label.unwrap_or_else(|| match ms { 0 => String::new(), ms => hhmm(ms / 1000) });
                let base = crate::settings::display_title(&doc.title.text);
                let title = if tag.is_empty() { base.to_string() } else { crate::localized_format!("{}（{}）", "{} ({})", base, tag) };
                doc.title = crate::model::RichText::plain(title);
                match self.store.create_from(&doc) {
                    Ok(new) => {
                        self.entries.set(self.store.list());
                        self.sort();
                        self.close_history(cx);
                        self.open(&new, window, cx);
                    }
                    Err(e) => self.set_history_error(e.to_string(), cx),
                }
            }
            Err(e) => self.set_history_error(e.to_string(), cx),
        }
    }

    pub fn delete_selected_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, name)) = self.history.as_ref().and_then(|h| h.selected.clone().map(|n| (h.id.clone(), n))) else { return };
        if let Err(e) = self.store.delete_version(&id, &name) { return self.set_history_error(e.to_string(), cx) }
        self.refresh_history(window, cx);
    }

    /// 命名版本: the row turns into an input; Enter or clicking away saves.
    pub fn start_version_rename(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(label) = self.history.as_ref().and_then(|h| h.versions.iter().find(|v| v.name == name)).map(|v| v.label.clone().unwrap_or_default()) else { return };
        let field = cx.new(|cx| {
            let mut f = TextField::new(crate::settings::tr("给这个版本起个名字", "Name this version"), cx);
            f.set_text(&label, cx);
            f.select_all_text(cx);
            f
        });
        let focus = field.focus_handle(cx);
        window.focus(&focus, cx);
        let subs = vec![
            cx.subscribe_in(&field, window, |w, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submit => w.finish_version_rename(true, window, cx),
                TextFieldEvent::Cancel => w.finish_version_rename(false, window, cx),
                _ => {}
            }),
            cx.on_blur(&focus, window, |w, window, cx| w.finish_version_rename(true, window, cx)),
        ];
        self.version_rename = Some(VersionRename { name: name.to_string(), field, _subs: subs });
        cx.notify();
    }

    fn finish_version_rename(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.version_rename.take() else { return };
        let Some(h) = self.history.as_ref() else { return };
        let id = h.id.clone();
        let text = if save { r.field.read(cx).text.clone() } else { String::new() };
        let label = (!text.trim().is_empty()).then(|| text.trim().to_string());
        if self.store.set_version_label(&id, &r.name, label.as_deref()).is_err() {
            return self.set_history_error(crate::settings::tr("重命名失败", "Could not rename").to_string(), cx);
        }
        if let Some(v) = self.history.as_mut().and_then(|h| h.versions.iter_mut().find(|v| v.name == r.name)) {
            v.label = label;
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn set_history_error(&mut self, msg: String, cx: &mut Context<Self>) {
        if let Some(h) = self.history.as_mut() { h.error = Some(msg) }
        cx.notify();
    }

    /// The mode bar above the page while a snapshot is on screen.
    pub(crate) fn render_history_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let h = self.history.as_ref()?;
        let p = self.preview.as_ref()?;
        let v = h.versions.iter().find(|v| v.name == p.name);
        let when = v.map_or_default(|v| format_time(v.ms / 1000));
        let title = match v.and_then(|v| v.label.clone()) {
            Some(label) => crate::localized_format!("正在查看「{}」· {}", "Viewing \u{201c}{}\u{201d} · {}", label, when),
            None => crate::localized_format!("正在查看 {} 的版本", "Viewing the version from {}", when),
        };
        let (added, removed) = v.and_then(|v| h.delta.get(&v.name).copied()).unwrap_or((0, 0));
        let has_marks = h.versions.iter().position(|x| x.name == p.name).is_some_and(|i| i + 1 < h.versions.len());
        let on = h.marks && has_marks;
        Some(
            div().id("history-banner").flex_none().h(px(BANNER_H)).px(px(16.)).pr(px(DRAWER_W + 24.)).flex().items_center().gap(px(10.))
                .bg(theme::ca(crate::theme::craft::O50, 0.6))
                .border_b_1().border_color(Theme::line_divider())
                .child(div().flex_none().text_size(px(12.)).text_color(Theme::text()).child(title))
                .child(div().flex_none().text_size(px(11.)).text_color(Theme::text_caption())
                    .child(crate::localized_format!("+{} 行 −{} 行", "+{} −{} lines", added, removed)))
                .child(div().flex_1())
                .child(
                    Button::ghost("history-marks").selected(on).disabled(!has_marks)
                        .when(has_marks, |d| d.on_click(cx.listener(|w, _, _, cx| w.toggle_history_marks(cx))))
                        .label(crate::settings::tr("显示改动", "Show changes")),
                )
                .child(Button::outline("history-back").sized(ButtonSize::Tool).label(crate::settings::tr("返回当前", "Back to now"))
                    .on_click(cx.listener(|w, _, _, cx| w.exit_preview(cx))))
                .child(Button::primary("history-restore").sized(ButtonSize::Tool).label(crate::settings::tr("恢复此版本", "Restore this version"))
                    .on_click(cx.listener(|w, _, window, cx| w.restore_selected(window, cx))))
                .into_any_element(),
        )
    }

    /// The 版本历史 drawer: a floating card like the app's other panels (popover surface, 13pt text,
    /// rows as menu rows). Header: title, count, 全部 / 已命名; then the versions by day; at the foot,
    /// 保存当前版本 and, for the version being previewed, 另存副本 and 删除.
    pub(crate) fn render_history(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let h = self.history.as_ref()?;
        let selected = h.selected.clone();
        let renaming = self.version_rename.as_ref().map(|r| (r.name.clone(), r.field.clone()));
        let visible: Vec<Version> = h.versions.iter().filter(|v| !h.named_only || v.label.is_some()).cloned().collect();
        let mut rows: Vec<AnyElement> = vec![];
        let mut day: Option<(i64, u32, u32)> = None;
        for (i, v) in visible.iter().enumerate() {
            let key = day_key(v.ms / 1000);
            if day != Some(key) {
                day = Some(key);
                rows.push(
                    div().flex_none().h(px(30.)).px(px(13.)).flex().items_end().pb(px(4.))
                        .text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(Theme::text_caption()).child(day_label(v.ms / 1000)).into_any_element(),
                );
            }
            let name = v.name.clone();
            let on = selected.as_deref() == Some(name.as_str());
            let group = format!("hrow-{i}");
            let delta = delta_text(&name, &h.delta);
            let label = v.label.clone();
            let editing = renaming.as_ref().is_some_and(|(n, _)| n == &name);
            let field = renaming.as_ref().filter(|(n, _)| n == &name).map(|(_, f)| f.clone());
            let click_name = name.clone();
            rows.push(
                div().id(("history-row", i)).group(group.clone()).flex_none().mx(px(dmenu::PAD))
                    .h(px(if label.is_some() { 48. } else { 36. })).px(px(8.))
                    .flex().flex_row().items_center().gap(px(10.)).rounded(px(crate::design::G.menu_row_r))
                    .when(on, |d| d.bg(crate::design::color::accent()))
                    .when(!editing && !on, |d| d.hover(|s| s.bg(Theme::fill_hover())))
                    .when(!editing, |d| d.cursor_pointer().on_click(cx.listener(move |w: &mut Workspace, _, window, cx| w.preview_version(&click_name, window, cx))))
                    .when(editing, |d| d.bg(Theme::fill_hover()))
                    .child(div().flex_none().w(px(40.)).text_size(px(13.)).text_color(crate::design::color::foreground())
                        .when(on, |d| d.font_weight(FontWeight::MEDIUM)).child(hhmm(v.ms / 1000)))
                    .child(match field {
                        Some(field) => div().flex_1().min_w_0().child(field).into_any_element(),
                        None => div().flex_1().min_w_0().flex().flex_col().justify_center()
                            .when_some(label.clone(), |d, l| d.child(div().text_size(px(13.)).line_height(px(20.)).font_weight(FontWeight::MEDIUM).text_color(Theme::text())
                                .overflow_hidden().whitespace_nowrap().text_ellipsis().child(l)))
                            .child(div().text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_caption())
                                .overflow_hidden().whitespace_nowrap().text_ellipsis().child(delta.clone()))
                            .into_any_element(),
                    })
                    .when(!editing, |d| d.child(
                        Button::ghost(("history-rename", i)).sized(ButtonSize::Xs).icon("pen-line")
                            .invisible().group_hover(group.clone(), |s| s.visible())
                            .on_click(cx.listener(move |w: &mut Workspace, _, window, cx| w.start_version_rename(&name, window, cx))),
                    ))
                    .into_any_element(),
            );
        }
        let count = h.versions.iter().filter(|v| v.label.is_some()).count();
        let has_sel = selected.is_some();
        // 全部 / 已命名: a segmented control
        let tab = |id: &'static str, text: String, on: bool| Segment::new(id, text).selected(on);
        let header = div().flex_none().pt(px(14.)).pb(px(8.)).px(px(14.)).flex().flex_col().gap(px(10.))
            .child(div().flex().items_center().gap(px(8.))
                .child(div().flex_1().text_size(px(15.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).child(crate::settings::tr("版本历史", "Version history")))
                .child(Button::ghost("history-close").sized(ButtonSize::Xs).icon("x").on_click(cx.listener(|w, _, _, cx| w.close_history(cx)))))
            .child(Segmented::new()
                .child(tab("history-all", crate::localized_format!("全部 {}", "All {}", h.versions.len()), !h.named_only)
                    .on_click(cx.listener(|w, _, _, cx| if w.history.as_ref().is_some_and(|h| h.named_only) { w.toggle_history_named(cx) })))
                .child(tab("history-named", crate::localized_format!("已命名 {}", "Named {}", count), h.named_only)
                    .on_click(cx.listener(|w, _, _, cx| if w.history.as_ref().is_some_and(|h| !h.named_only) { w.toggle_history_named(cx) }))));
        let empty = |title: &'static str, hint: &'static str| div().flex_1().flex().flex_col().items_center().justify_center().gap(px(6.)).px(px(28.)).pb(px(40.))
            .child(crate::ui::icon("restore", 28., Theme::text_placeholder()))
            .child(div().mt(px(4.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(Theme::text()).child(title))
            .child(div().text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_caption()).text_center().child(hint));
        let list = if visible.is_empty() {
            if h.versions.is_empty() {
                empty(crate::settings::tr("还没有历史版本", "No versions yet"), crate::settings::tr("编辑时每 5 分钟自动保存一份；保留的份数在 设置 › 文档库 里调。", "A version is kept every 5 minutes as you edit. How many are kept is in Settings › Library."))
            } else {
                empty(crate::settings::tr("还没有命名版本", "Nothing named yet"), crate::settings::tr("把鼠标移到某一版上，点铅笔给它起个名字。", "Hover a version and click the pencil to name it."))
            }.into_any_element()
        } else {
            div().id("history-list").flex_1().min_h_0().overflow_y_scroll().track_scroll(&h.list_scroll).flex().flex_col().pb(px(6.)).children(rows).child(crate::design::scroll::bar("history-list", &h.list_scroll, 0.)).into_any_element()
        };
        let act = |id: &'static str, text: &'static str| Button::ghost(id).label(text);
        let footer = div().flex_none().border_t_1().border_color(Theme::line_divider()).p(px(12.)).flex().items_center().gap(px(4.))
            .child(Button::outline("history-snapshot").sized(ButtonSize::Tool).label(crate::settings::tr("保存当前版本", "Save a version"))
                .on_click(cx.listener(|w, _, window, cx| w.save_version_now(window, cx))))
            .child(div().flex_1())
            // these act on the version being previewed: shown only while there is one
            .when(has_sel, |d| d
                .child(act("history-copy", crate::settings::tr("另存副本", "Save as copy")).on_click(cx.listener(|w, _, window, cx| w.version_as_copy(window, cx))))
                .child(Button::destructive("history-delete").sized(ButtonSize::Tool).label(crate::settings::tr("删除", "Delete"))
                    .on_click(cx.listener(|w, _, window, cx| w.delete_selected_version(window, cx)))));
        Some(
            Popover::new("history-drawer").absolute().top(px(HEADER_H + 8.)).right(px(8.)).bottom(px(8.)).w(px(DRAWER_W))
                .flex().flex_col().overflow_hidden()
                .child(header)
                .children(h.error.clone().map(|e| div().flex_none().mx(px(14.)).mb(px(6.)).text_size(px(12.)).line_height(px(18.)).text_color(Theme::danger()).child(e)))
                .child(list)
                .child(footer)
                .into_any_element(),
        )
    }
}
