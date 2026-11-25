//! 笔记 cells (docs/design/DATA_TABLE_DIRECTION.md 3.5): a cell is one note,
//! a document of the library, and holds its path relative to the table file
//! (`field::note_href`). Here: what a cell shows (the note's title, 空笔记,
//! 找不到笔记), making a note for an empty cell, pointing a cell at a note
//! there already is, the note itself edited in 记录详情 (a whole `Editor`,
//! saved through the library like any document), and the notes that go to
//! the 回收站 with their records and come back with undo.
//!
//! The library is reached through `SharedStore` (the app's global); the
//! workspace hears what changed through `TableEvent`.

use super::style::*;
use super::*;
use crate::editor::view::{Editor, EditorEvent};
use crate::shared_store::SharedStore;
use crate::ui::ui_text;

fn label(s: &str) -> String { crate::settings::label(s) }

/// A document of the table's library, as a 笔记 cell needs it.
#[derive(Clone)]
pub(crate) struct NoteInfo {
    pub id: String,
    pub title: String,
    /// Nothing written in it yet.
    pub empty: bool,
}

/// Where the table is: its library's folder on disk and key, its own path from there, the folder it
/// is in (`table_parent`), and where its notes go: `notes`, a folder's path, and `parent`, that folder
/// once it is there (`ensure_folder`). A table's notes are kept beside it in a plain folder
/// 「<表名> · 笔记」, made when the first one is; a table still in a folder of its own name (as
/// older builds kept it, its 正文) keeps its notes there.
#[derive(Clone)]
pub(crate) struct Place { top: PathBuf, key: String, rel: String, table_parent: Option<String>, notes: String, parent: Option<String> }

/// The folder beside table `rel` its notes go in: 「书单 · 笔记」 for `书单.sqlite`; the table's own
/// folder when it is the 正文 of one named as it is (`书单/书单.sqlite`, an older build's way).
/// The word a notes folder's name ends with (「书单 · 笔记」, "Books · Notes").
fn notes_word() -> &'static str { crate::settings::tr("笔记", "Notes") }

fn notes_dir(rel: &str) -> String {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    if dir.rsplit('/').next() == Some(stem) { return dir.to_string() }
    let name = format!("{stem} · {}", notes_word());
    if dir.is_empty() { name } else { format!("{dir}/{name}") }
}

/// The note open in 记录详情.
pub(crate) struct OpenNote {
    pub cell: CellId,
    pub id: String,
    pub editor: Entity<Editor>,
    dirty: bool,
    save: Option<Task<()>>,
    _sub: Subscription,
}

/// Records or fields about to go, whose notes are asked about first.
pub(crate) struct Ask {
    records: Vec<RecId>,
    fields: Vec<FieldId>,
    /// The notes that would go with them: id, title.
    notes: Vec<(String, String)>,
}

#[derive(Default)]
pub(crate) struct NotesUi {
    /// The library's documents by their path from its top, as of the last look (`refresh`).
    index: Rc<HashMap<String, NoteInfo>>,
    place: Option<Place>,
    /// The document each 笔记 cell was last seen pointing at: a note renamed or moved keeps its cell.
    ids: HashMap<CellId, String>,
    /// Notes this view made, or sent to the 回收站 with their records, with where they are (were;
    /// `refresh` and `save` keep it right): the undo or redo that takes the last cell off one sends
    /// it to the 回收站, the one that points a cell at it again brings it back (`after_history`).
    mine: HashMap<String, String>,
    gone: HashSet<String>,
    pub open: Option<OpenNote>,
    pub ask: Option<Ask>,
    /// 记录详情's width once dragged, and the drag: where it was pressed, the width then.
    pub width: Option<f32>,
    pub grip: Option<(f32, f32)>,
}

impl NotesUi {
    /// The library hasn't been looked at yet.
    pub fn unseen(&self) -> bool { self.place.is_none() && self.index.is_empty() }
}

fn store(cx: &App) -> Option<SharedStore> { cx.has_global::<SharedStore>().then(|| cx.global::<SharedStore>().clone()) }

/// A path as the library writes them.
fn slashed(p: &Path) -> String { p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/") }

fn place(v: &TableView, cx: &App) -> Option<Place> {
    // a table of its own in the library: not one kept inside a page (`.assets`), not a version being looked at
    if !v.standalone { return None; }
    let st = v.state.read(cx);
    if st.read_only { return None; }
    let store = store(cx)?;
    let key = store.folder_of_path(&st.path)?;
    let top = store.top_path(&key)?;
    let rel = slashed(st.path.strip_prefix(&top).ok()?);
    if rel.split('/').any(|p| p.starts_with('.')) { return None; }
    let table_parent = store.listed(|tops| tops.iter().flat_map(|r| &r.1).find(|e| e.meta.rel == rel && e.meta.folder.is_none()).and_then(|e| e.meta.parent.clone()));
    // the notes folder as it is now: where the table's notes are already (a table renamed keeps
    // using the folder its notes are in), else beside it by its name
    let suffix = format!(" · {}", notes_word());
    let own_dir = |n: &str| n.rsplit_once('/').map(|(d, _)| d.to_string()).filter(|d| d.ends_with(&suffix));
    let in_cells = || st.doc.base.table(v.table).and_then(|t| t.fields.iter().filter(|f| f.ty == FieldType::Note)
        .find_map(|f| t.records.iter().find_map(|r| match r.get(f.id) { Value::Text(href) => field::note_rel(&rel, href).and_then(|n| own_dir(&n)), _ => None })));
    let notes = v.notes.mine.values().find_map(|n| own_dir(n)).or_else(in_cells).unwrap_or_else(|| notes_dir(&rel));
    let parent = store.listed(|tops| tops.iter().filter(|r| r.0 == key).flat_map(|r| &r.1).find(|e| e.meta.folder.is_some() && e.meta.rel == notes).map(|e| e.id.clone()));
    Some(Place { top, key, rel, table_parent, notes, parent })
}

/// Where a new note goes: the table's notes folder (`Place::notes`), made now beside the table when it
/// isn't there yet. The table itself stays where it is.
pub(crate) fn ensure_folder(v: &mut TableView, cx: &mut Context<TableView>) -> Option<Place> {
    let mut h = place(v, cx)?;
    if h.parent.is_some() { return Some(h) }
    let mut store = store(cx)?;
    let name = h.notes.rsplit('/').next().unwrap_or(&h.notes).to_string();
    // at the top of its top-level folder, the folder goes in that one's row
    let parent = h.table_parent.clone().unwrap_or_else(|| crate::shared_store::top_row(&h.key));
    match store.create_folder(&name, Some(&parent)) {
        Ok(id) => {
            h.parent = Some(id.clone());
            // the name it got (a taken one is numbered): where the notes are written from now
            if let Some(path) = store.file_path(&id) && let Ok(rel) = path.strip_prefix(&h.top) { h.notes = slashed(rel); }
            cx.emit(TableEvent::LibraryChanged);
        }
        Err(e) => { log::error!("笔记文件夹没能建立: {e:#}"); return None; }
    }
    v.notes.place = Some(h.clone());
    Some(h)
}

/// 笔记 columns can be added here: a table that is a file of the library.
pub(crate) fn allowed(v: &TableView, cx: &App) -> bool { place(v, cx).is_some() }

/// Look at the library again: the documents a cell can point at, and cells
/// whose note was renamed or moved follow it.
pub(crate) fn refresh(v: &mut TableView, cx: &mut Context<TableView>) {
    let Some(h) = place(v, cx) else { v.notes.place = None; return };
    let Some(store) = store(cx) else { return };
    let key = store.folder_of_path(&h.top.join(&h.rel));
    let mut index = HashMap::new();
    store.listed(|tops| {
        for e in tops.iter().filter(|r| Some(&r.0) == key.as_ref()).flat_map(|r| &r.1) {
            if e.meta.folder.is_some() || e.meta.local() || e.meta.kind != crate::store::DocKind::Doc || e.meta.rel.is_empty() { continue; }
            index.insert(e.meta.rel.clone(), NoteInfo { id: e.id.clone(), title: e.title.clone(), empty: e.body.trim().is_empty() });
        }
    });
    // the note being edited is ahead of the list
    if let Some(o) = &v.notes.open {
        let doc = &o.editor.read(cx).core.doc;
        if let Some(n) = index.values_mut().find(|n| n.id == o.id) { n.title = doc.title.text.clone(); n.empty = doc.plain_text().trim().is_empty(); }
    }
    let changed = index.len() != v.notes.index.len() || index.iter().any(|(k, n)| v.notes.index.get(k).is_none_or(|o| o.id != n.id || o.title != n.title || o.empty != n.empty));
    let by_id: HashMap<&str, &str> = index.iter().map(|(rel, n)| (n.id.as_str(), rel.as_str())).collect();
    // our notes renamed or moved elsewhere (the sidebar) are still ours, where they are now
    for (id, rel) in v.notes.mine.iter_mut() { if let Some(now) = by_id.get(id.as_str()).filter(|now| **now != rel.as_str()) { *rel = now.to_string(); } }
    // cells whose note is elsewhere now, and what every cell points at
    let tid = v.table;
    let mut moved = vec![];
    let mut ids = HashMap::new();
    if let Some(t) = v.state.read(cx).doc.base.table(tid) {
        for f in t.fields.iter().filter(|f| f.ty == FieldType::Note) {
            for r in &t.records {
                let Value::Text(href) = r.get(f.id) else { continue };
                let cell = (r.id, f.id);
                let at = field::note_rel(&h.rel, href);
                match at.as_deref().and_then(|rel| index.get(rel)) {
                    Some(n) => { ids.insert(cell, n.id.clone()); }
                    // not where the cell says: where the document it was is now
                    None => if let Some(rel) = v.notes.ids.get(&cell).and_then(|id| by_id.get(id.as_str()).map(|rel| (id, *rel))) {
                        ids.insert(cell, rel.0.clone());
                        moved.push((r.id, f.id, Value::Text(field::note_href(&h.rel, rel.1))));
                    },
                }
            }
        }
    }
    v.notes.ids = ids;
    v.notes.place = Some(h);
    if changed { v.notes.index = Rc::new(index); cx.notify(); }
    if !moved.is_empty() { v.edit(cx, |d| d.set_cells(tid, moved)); }
}

/// What a 笔记 cell points at. None: nothing where it says (找不到笔记).
pub(crate) fn info<'a>(v: &'a TableView, href: &str) -> Option<&'a NoteInfo> {
    let h = v.notes.place.as_ref()?;
    v.notes.index.get(&field::note_rel(&h.rel, href)?)
}

fn title_of(n: &NoteInfo) -> String { if n.title.trim().is_empty() { label("未命名文档") } else { n.title.clone() } }

/// A 笔记 cell as the grid and 记录详情 show it: the note's title (and 空笔记 while nothing is written in it), or 找不到笔记.
pub(crate) fn cell_view(v: &TableView, href: &str) -> AnyElement {
    let row = div().w_full().flex().items_center().gap(px(6.)).overflow_hidden().line_height(px(20.));
    match info(v, href) {
        Some(n) => row.child(icon("doc", 14., Theme::icon()))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title_of(n))))
            .when(n.empty, |d| d.child(div().flex_none().text_color(Theme::text_placeholder()).child(label("空笔记"))))
            .into_any_element(),
        None => row.child(icon("doc", 14., Theme::danger().opacity(0.6)))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(Theme::danger().opacity(0.7)).child(label("找不到笔记")))
            .into_any_element(),
    }
}

/// The note editor has the keys: the table's own shortcuts stand back.
pub(crate) fn typing(v: &TableView, window: &Window, cx: &App) -> bool {
    v.notes.open.as_ref().is_some_and(|o| o.editor.read(cx).focus.contains_focused(window, cx))
}

/// A 笔记 cell was opened (a click on the selected cell, a double click, Enter, its row in 记录详情):
/// its note shows in 记录详情, made first if the cell is empty.
pub(crate) fn open_cell(v: &mut TableView, cell: CellId, at: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    refresh(v, cx);
    // a table that got its 笔记 column before tables had folders moves into one now
    let Some(h) = ensure_folder(v, cx) else { return };
    let got = v.state.read(cx).doc.base.table(v.table).and_then(|t| Some((t.record(cell.0)?.get(cell.1).clone(), t.title(t.record(cell.0)?))));
    let Some((val, title)) = got else { return };
    let id = match &val {
        Value::Text(href) => info(v, href).map(|n| n.id.clone()),
        _ => make(v, &h, cell, &title, cx),
    };
    record::open(v, cell.0, window, cx);
    match id {
        Some(id) => show(v, cell, &id, window, cx),
        // 找不到笔记: point the cell at another
        None => { close(v, cx); open_pick(v, cell, at, window, cx); }
    }
}

/// A new note beside the table, titled as the record is, for an empty cell. Undo empties the cell
/// again and the note goes to the 回收站 (`after_history`).
fn make(v: &mut TableView, h: &Place, cell: CellId, title: &str, cx: &mut Context<TableView>) -> Option<String> {
    let mut store = store(cx)?;
    let mut doc = crate::model::Document::default();
    let title = title.trim();
    doc.title = crate::model::RichText::plain(if title.is_empty() { label("未命名文档") } else { title.to_string() });
    let made = store.create_from_in(&doc, h.parent.as_deref()).and_then(|id| {
        let path = store.file_path(&id).ok_or_else(|| anyhow::anyhow!("no file"))?;
        Ok((id, slashed(path.strip_prefix(&h.top)?)))
    });
    let (id, rel) = match made {
        Ok(m) => m,
        Err(e) => { log::error!("笔记没能建立: {e:#}"); return None; }
    };
    let tid = v.table;
    v.edit(cx, |d| d.set_cells(tid, vec![(cell.0, cell.1, Value::Text(field::note_href(&h.rel, &rel)))]));
    v.notes.mine.insert(id.clone(), rel.clone());
    v.notes.ids.insert(cell, id.clone());
    // known at once, before the library is listed again
    let mut index = (*v.notes.index).clone();
    index.insert(rel, NoteInfo { id: id.clone(), title: doc.title.text.clone(), empty: true });
    v.notes.index = Rc::new(index);
    cx.emit(TableEvent::LibraryChanged);
    Some(id)
}

/// Show document `id` in 记录详情 as `cell`'s note.
fn show(v: &mut TableView, cell: CellId, id: &str, window: &mut Window, cx: &mut Context<TableView>) {
    if v.notes.open.as_ref().is_some_and(|o| o.cell == cell && o.id == id) { return; }
    close(v, cx);
    let Some(store) = store(cx) else { return };
    let doc = match store.load_for_edit(id, false) {
        Ok(d) => d,
        Err(e) => { log::error!("笔记打不开 {id}: {e:#}"); return; }
    };
    let meta = store.listed(|tops| tops.iter().flat_map(|r| &r.1).find(|e| e.id == id).map(|e| e.meta.clone()));
    let editor = cx.new(|cx| {
        let mut e = Editor::new(doc, meta.as_ref().map_or_else(crate::time::now, |m| m.modified), Default::default(), window, cx);
        if let Some(m) = &meta { e.created = m.created; }
        // the panel is narrow: no outline beside the text
        e.cat_pinned = false;
        e
    });
    let sub = cx.subscribe_in(&editor, window, |v, _, ev: &EditorEvent, _, cx| match ev {
        EditorEvent::Changed => changed(v, cx),
        EditorEvent::Save => save(v, cx),
        EditorEvent::OpenDoc(id) | EditorEvent::OpenDocAt(id, _) => { save(v, cx); cx.emit(TableEvent::OpenNote(id.clone())); }
        _ => {}
    });
    v.notes.open = Some(OpenNote { cell, id: id.to_string(), editor, dirty: false, save: None, _sub: sub });
    cx.notify();
}

/// The note was edited: saved a moment after the last change.
fn changed(v: &mut TableView, cx: &mut Context<TableView>) {
    let Some(o) = v.notes.open.as_mut() else { return };
    o.dirty = true;
    o.save = Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(Duration::from_millis(800)).await;
        let _ = this.update(cx, save);
    }));
    cx.notify();
}

/// Write the open note now, if it changed. Its file is named after its title: a new title moves
/// it, and the cell follows.
pub(crate) fn save(v: &mut TableView, cx: &mut Context<TableView>) {
    let Some(o) = v.notes.open.as_mut().filter(|o| o.dirty) else { return };
    o.save = None;
    let (id, cell, editor) = (o.id.clone(), o.cell, o.editor.clone());
    let Some(store) = store(cx) else { return };
    let doc = editor.read(cx).core.doc.clone();
    match store.with(Some(&id), |s| s.save_editor(&id, &doc)) {
        Ok(meta) => {
            if let Some(o) = v.notes.open.as_mut() { o.dirty = false; }
            editor.update(cx, |e, _| e.modified = meta.modified);
            // the cell follows the file; the index is put right at once so the cell never reads 找不到笔记 in between
            if let Some(h) = v.notes.place.clone() {
                let mut index = (*v.notes.index).clone();
                index.retain(|_, n| n.id != id);
                index.insert(meta.rel.clone(), NoteInfo { id: id.clone(), title: doc.title.text.clone(), empty: doc.plain_text().trim().is_empty() });
                v.notes.index = Rc::new(index);
                if let Some(rel) = v.notes.mine.get_mut(&id) { *rel = meta.rel.clone(); }
                let (tid, href) = (v.table, field::note_href(&h.rel, &meta.rel));
                let stale = v.state.read(cx).doc.base.table(tid).and_then(|t| t.record(cell.0)).is_some_and(|r| matches!(r.get(cell.1), Value::Text(old) if *old != href));
                if stale { v.edit(cx, |d| d.set_cells(tid, vec![(cell.0, cell.1, Value::Text(href))])); }
            }
            let saved = NoteSaved { id, title: doc.title.text.clone(), icon: doc.icon.clone(), body: doc.plain_text().into(), meta };
            store.update_listed(&saved.id, |e| { (e.title, e.icon, e.body) = (saved.title.clone(), saved.icon.clone(), saved.body.clone()); e.meta.rel = saved.meta.rel.clone(); e.meta.modified = saved.meta.modified; });
            cx.emit(TableEvent::NoteSaved(Box::new(saved)));
        }
        Err(e) => log::error!("笔记保存失败 {id}: {e:#}"),
    }
    cx.notify();
}

/// Leave the open note (another record, the panel closed): saved first.
pub(crate) fn close(v: &mut TableView, cx: &mut Context<TableView>) {
    save(v, cx);
    if let Some(o) = v.notes.open.take() {
        if let Some(store) = store(cx) { store.forget_loaded(&o.id); }
        cx.notify();
    }
}

/// 整页打开: the note over the whole pane, as any document.
fn open_full(v: &mut TableView, id: String, cx: &mut Context<TableView>) {
    save(v, cx);
    cx.emit(TableEvent::OpenNote(id));
}

// ---- 指向已有笔记 ----

pub(crate) fn open_pick(v: &mut TableView, cell: CellId, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    refresh(v, cx);
    v.aux.update(cx, |t, cx| { t.size = text_sm(); t.line_height = 20.; t.set_placeholder(label("搜索笔记"), cx); t.set_text("", cx); });
    window.focus(&v.aux.focus_handle(cx), cx);
    v.rename = None;
    v.open_popup(pos, Popup::NotePick { rec: cell.0, field: cell.1 }, cx);
}

/// The library's documents, narrowed by what is typed; a click points the cell at one.
pub(crate) fn pick_popup(v: &TableView, rec: RecId, fid: FieldId, cx: &Context<TableView>) -> AnyElement {
    let query = v.aux.read(cx).text.trim().to_lowercase();
    let mut notes: Vec<(&String, &NoteInfo)> = v.notes.index.iter().filter(|(_, n)| query.is_empty() || n.title.to_lowercase().contains(&query)).collect();
    notes.sort_by(|a, b| field::text_cmp(&a.1.title, &b.1.title).then(a.0.cmp(b.0)));
    let mut list = div().id("tb-note-list").max_h(px(320.)).overflow_y_scroll().track_scroll(&crate::design::scroll::handle("tb-note-list")).flex().flex_col();
    let none = notes.is_empty();
    for (i, (rel, n)) in notes.into_iter().take(200).enumerate() {
        let (rel, id) = (rel.clone(), n.id.clone());
        // which folder it is in, for notes of one name
        let place = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        list = list.child(list_row(("tb-note", i))
            .child(icon("doc", 14., Theme::icon()))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title_of(n))))
            .when(!place.is_empty(), |d| d.child(div().flex_none().max_w(px(120.)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(12.)).text_color(Theme::text_placeholder()).child(ui_text(place.clone()))))
            .on_click(cx.listener(move |v, _, window, cx| {
                let Some(h) = v.notes.place.clone() else { return };
                let tid = v.table;
                v.edit(cx, |d| d.set_cells(tid, vec![(rec, fid, Value::Text(field::note_href(&h.rel, &rel)))]));
                v.notes.ids.insert((rec, fid), id.clone());
                v.close_popup(window, cx);
                if v.expanded == Some(rec) { show(v, (rec, fid), &id, window, cx); }
            })));
    }
    if none { list = list.child(div().px(px(12.)).py(px(10.)).text_color(Theme::text_placeholder()).child(label("没有笔记"))); }
    crate::design::Popover::new("tb-note-pick").occlude().w(px(320.)).py(px(6.))
        .on_mouse_down_out(cx.listener(|v, _, window, cx| v.close_popup(window, cx)))
        .child(div().px(px(12.)).pb(px(6.)).text_size(px(12.)).text_color(Theme::text_caption()).child(label("指向已有笔记")))
        .child(div().px(px(8.)).pb(px(6.)).child(input_box("tb-note-search").child(icon("search", 14., Theme::text_placeholder())).gap(px(6.)).child(div().flex_1().child(v.aux.clone()))))
        .child(list.child(crate::design::scroll::bar("tb-note-list", &crate::design::scroll::handle("tb-note-list"), 0.)))
        .into_any_element()
}

// ---- 记录详情 ----

/// 记录详情's width: as dragged, else the usual.
pub(crate) fn panel_w(v: &TableView) -> f32 { v.notes.width.unwrap_or(RECORD_W) }

/// The panel's left edge: dragged, the panel is wider or narrower.
pub(crate) fn grip(v: &TableView, cx: &Context<TableView>) -> impl IntoElement {
    let w = panel_w(v);
    let dragging = v.notes.grip.is_some();
    div().id("tb-rec-grip").absolute().top_0().bottom_0().left(px(-3.)).w(px(6.)).cursor(CursorStyle::ResizeLeftRight).group("tb-rec-grip")
        .child(div().mx_auto().w(px(2.)).h_full().when(dragging, |d| d.bg(Theme::primary())).when(!dragging, |d| d.group_hover("tb-rec-grip", |s| s.bg(Theme::primary()))))
        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            // a double click puts it back
            if ev.click_count >= 2 { v.notes.width = None; } else { v.notes.grip = Some((f32::from(ev.position.x), w)); }
            cx.notify();
        }))
        // the drag follows the pointer anywhere in the window
        .when(dragging, |d| {
            let (a, b) = (cx.weak_entity(), cx.weak_entity());
            d.child(canvas(|_, _, _| {}, move |_, _, window, _| {
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| if phase == DispatchPhase::Capture {
                    let _ = a.update(cx, |v, cx| {
                        let Some((x0, w0)) = v.notes.grip else { return };
                        let max = (f32::from(window.viewport_size().width) - 360.).max(RECORD_W);
                        v.notes.width = Some((w0 + x0 - f32::from(ev.position.x)).clamp(360., max));
                        cx.notify();
                    });
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Capture { let _ = b.update(cx, |v, cx| { v.notes.grip = None; cx.notify(); }); });
            }).absolute().size_0())
        })
}

/// A 笔记 field's value in 记录详情: its note's title (the one open in the panel marked), or what to do about there being none.
pub(crate) fn row_value(v: &TableView, cell: CellId, val: &Value) -> AnyElement {
    match val {
        Value::Text(href) => {
            let open = v.notes.open.as_ref().is_some_and(|o| o.cell == cell);
            div().w_full().flex().items_center().gap(px(6.)).child(div().flex_1().min_w_0().child(cell_view(v, href)))
                .when(open, |d| d.child(icon("chevron-down", 13., Theme::icon())))
                .into_any_element()
        }
        _ => div().text_color(Theme::text_placeholder()).child(label("新建笔记")).into_any_element(),
    }
}

/// The panel a 笔记 cell opens: the note alone (the record's fields are the row it came from). Its
/// head is a page's top bar (`Workspace::top_bar`): as high, the path in its crumbs (the table, then
/// the note's title), its actions the same 32pt icons at the right; a page's 搜索 and 新建 have no
/// place here, 指向已有笔记, 整页打开 and closing do.
pub(crate) fn pane(v: &TableView, rec: RecId, cx: &Context<TableView>) -> Option<AnyElement> {
    let o = v.notes.open.as_ref().filter(|o| o.cell.0 == rec)?;
    let (cell, id) = (o.cell, o.id.clone());
    let table = v.state.read(cx).doc.base.table(v.table).map(|t| t.name.clone()).unwrap_or_default();
    let title = crate::settings::display_title(&o.editor.read(cx).core.doc.title.text);
    let seg = crate::ui::header_icon;
    use crate::ui::TipExt;
    let sep = || div().mx(px(3.)).flex_none().child(icon("chevron-right", 12., Theme::icon()));
    let crumbs = div().h(px(20.)).min_w_0().overflow_hidden().flex().items_center().text_size(px(13.)).line_height(px(20.))
        .child(div().id("tb-note-crumb").flex_none().max_w(px(200.)).px(px(4.)).rounded(px(4.)).overflow_hidden().whitespace_nowrap().text_ellipsis()
            .text_color(Theme::text_caption()).cursor_pointer().hover(|s| s.text_color(crate::theme::Theme::primary_500())).child(ui_text(table))
            .on_click(cx.listener(|v, _, window, cx| record::close(v, window, cx))))
        .child(sep())
        .child(div().min_w_0().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title)));
    let actions = div().flex_none().h(px(crate::app::HEADER_ACTIONS_H)).flex().items_center().gap(px(4.))
        .child(seg("tb-note-point", "link").tip(&label("指向已有笔记"), None).on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| {
            let p = ev.position();
            open_pick(v, cell, point(p.x - px(280.), p.y + px(18.)), window, cx);
        })))
        .child(seg("tb-note-full", "expand").tip(&label("整页打开"), None).on_click(cx.listener(move |v, _, _, cx| open_full(v, id.clone(), cx))))
        .child(seg("tb-rec-close", "x").tip(&label("关闭"), None).on_click(cx.listener(|v, _, window, cx| record::close(v, window, cx))));
    let head = div().id("tb-note-head").flex_none().h(px(crate::app::HEADER_H)).pl(px(18.)).pr(px(crate::ui::header_right_inset())).flex().items_center()
        .child(crumbs).child(div().flex_1().min_w(px(16.))).child(actions);
    Some(div().flex_1().min_h_0().flex().flex_col()
        .child(head)
        .child(div().id("tb-note-editor").flex_1().min_h_0().overflow_hidden().child(o.editor.clone()))
        .into_any_element())
}

// ---- deleting ----

/// The notes that are the table's own (in its notes folder) among these records' or fields' cells.
fn owned(v: &TableView, records: &[RecId], fields: &[FieldId], cx: &App) -> Vec<(String, String)> {
    let Some(h) = v.notes.place.as_ref() else { return vec![] };
    let dir = h.notes.as_str();
    let st = v.state.read(cx);
    let Some(t) = st.doc.base.table(v.table) else { return vec![] };
    let mut out: Vec<(String, String)> = vec![];
    for f in t.fields.iter().filter(|f| f.ty == FieldType::Note && (fields.is_empty() || fields.contains(&f.id))) {
        for r in t.records.iter().filter(|r| records.is_empty() || records.contains(&r.id)) {
            let Value::Text(href) = r.get(f.id) else { continue };
            let Some(rel) = field::note_rel(&h.rel, href) else { continue };
            if rel.rsplit_once('/').map_or("", |(d, _)| d) != dir { continue; }
            if let Some(n) = v.notes.index.get(&rel) && !out.iter().any(|o| o.0 == n.id) { out.push((n.id.clone(), title_of(n))); }
        }
    }
    out
}

/// Delete records. Notes of theirs that are the table's own are asked about first.
pub(crate) fn remove_records(v: &mut TableView, ids: Vec<RecId>, window: &mut Window, cx: &mut Context<TableView>) {
    if ids.is_empty() { return; }
    refresh(v, cx);
    let notes = owned(v, &ids, &[], cx);
    if notes.is_empty() { return finish(v, ids, vec![], false, window, cx); }
    v.notes.ask = Some(Ask { records: ids, fields: vec![], notes });
    cx.notify();
}

/// Delete fields; a 笔记 column's notes are asked about first.
pub(crate) fn remove_fields(v: &mut TableView, ids: Vec<FieldId>, window: &mut Window, cx: &mut Context<TableView>) {
    if ids.is_empty() { return; }
    refresh(v, cx);
    let notes = owned(v, &[], &ids, cx);
    if notes.is_empty() { return finish(v, vec![], ids, false, window, cx); }
    v.notes.ask = Some(Ask { records: vec![], fields: ids, notes });
    cx.notify();
}

/// Go ahead: the records or fields are deleted; with `notes`, their notes go to the 回收站 (undo brings both back).
fn finish(v: &mut TableView, records: Vec<RecId>, fields: Vec<FieldId>, notes: bool, window: &mut Window, cx: &mut Context<TableView>) {
    let ask = v.notes.ask.take();
    if v.notes.open.as_ref().is_some_and(|o| records.contains(&o.cell.0) || fields.contains(&o.cell.1)) { close(v, cx); }
    if notes && let Some(a) = &ask { trash(v, a.notes.iter().map(|n| n.0.clone()).collect(), cx); }
    let tid = v.table;
    if !records.is_empty() {
        if v.expanded.is_some_and(|r| records.contains(&r)) { v.expanded = None; }
        v.edit(cx, |d| d.remove_records(tid, &records));
        for r in &records { v.rows_sel.remove(r); }
        if v.sel.is_some_and(|s| records.contains(&s.anchor.0) || records.contains(&s.head.0)) { v.sel = None; }
    }
    if !fields.is_empty() {
        v.edit(cx, |d| d.remove_fields(tid, &fields));
        v.col_sel.clear();
    }
    if !v.input.read(cx).is_focused(window) { window.focus(&v.focus, cx); }
    cx.notify();
}

/// Send notes to the 回收站, remembering them for undo.
fn trash(v: &mut TableView, ids: Vec<String>, cx: &mut Context<TableView>) {
    let Some(store) = store(cx) else { return };
    let mut index = (*v.notes.index).clone();
    for id in ids {
        let Some(rel) = index.iter().find(|(_, n)| n.id == id).map(|(rel, _)| rel.clone()) else { continue };
        match store.with(Some(&id), |s| s.delete(&id, None)) {
            Ok(()) => { index.remove(&rel); v.notes.mine.insert(id.clone(), rel); v.notes.gone.insert(id); }
            Err(e) => log::error!("笔记没能移进回收站 {id}: {e:#}"),
        }
    }
    v.notes.index = Rc::new(index);
    cx.emit(TableEvent::LibraryChanged);
}

/// The document a 笔记 cell points at, by id: the one at its path; else the one the cell was last
/// seen at (renamed or moved since, the cell not yet following); else a note of ours in the 回收站
/// that was there.
fn target<'a>(n: &'a NotesUi, cell: CellId, rel: Option<&str>) -> Option<&'a String> {
    rel.and_then(|rel| n.index.get(rel)).map(|i| &i.id)
        .or_else(|| n.ids.get(&cell))
        .or_else(|| rel.and_then(|rel| n.mine.iter().find(|(id, at)| at.as_str() == rel && n.gone.contains(*id))).map(|(id, _)| id))
}

/// The documents the table's 笔记 cells point at now, by id (`target`): asked before an undo or
/// redo, for `after_history` to see what the step did.
pub(crate) fn pointed(v: &TableView, cx: &App) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(h) = v.notes.place.as_ref().filter(|_| !v.notes.mine.is_empty()) else { return out };
    let Some(t) = v.state.read(cx).doc.base.table(v.table) else { return out };
    for f in t.fields.iter().filter(|f| f.ty == FieldType::Note) {
        for r in &t.records {
            let Value::Text(href) = r.get(f.id) else { continue };
            if let Some(id) = target(&v.notes, (r.id, f.id), field::note_rel(&h.rel, href).as_deref()) { out.insert(id.clone()); }
        }
    }
    out
}

/// What an undo or redo did to our notes, from what the cells pointed at `before` and `after` it:
/// (back from the 回收站, into it). Only the step that itself takes the last cell off a note sends
/// it away, and only the one that points a cell at it again brings it back: a note whose cell was
/// emptied or pointed elsewhere by hand stays through the undo of anything else.
fn history_moves(n: &NotesUi, before: &HashSet<String>, after: &HashSet<String>) -> (Vec<String>, Vec<String>) {
    let (mut back, mut away) = (vec![], vec![]);
    for id in n.mine.keys() {
        match (before.contains(id), after.contains(id), n.gone.contains(id)) {
            (false, true, true) => back.push(id.clone()),
            (true, false, false) => away.push(id.clone()),
            _ => {}
        }
    }
    (back, away)
}

/// After undo or redo (`before`: `pointed` as it was): a note this view made or sent to the 回收站
/// goes there with the step that took its last cell away, and comes back with the one that
/// points a cell at it again.
pub(crate) fn after_history(v: &mut TableView, before: HashSet<String>, cx: &mut Context<TableView>) {
    if v.notes.mine.is_empty() { return; }
    let Some(mut store) = store(cx) else { return };
    let mut changed = false;
    let (back, away) = history_moves(&v.notes, &before, &pointed(v, cx));
    for id in back {
        match store.restore(&id) {
            Ok(_) => { v.notes.gone.remove(&id); changed = true; }
            Err(e) => log::error!("笔记没能从回收站恢复 {id}: {e:#}"),
        }
    }
    if !away.is_empty() {
        if v.notes.open.as_ref().is_some_and(|o| away.contains(&o.id)) { close(v, cx); }
        for id in away {
            match store.with(Some(&id), |s| s.delete(&id, None)) {
                Ok(()) => { v.notes.gone.insert(id); changed = true; }
                Err(e) => log::error!("笔记没能移进回收站 {id}: {e:#}"),
            }
        }
    }
    if changed {
        // the lists are read again by the workspace; ours follows from them on the next look
        cx.emit(TableEvent::LibraryChanged);
        let this = cx.weak_entity();
        cx.defer(move |cx| { let _ = this.update(cx, refresh); });
    }
}

/// 「同时删除笔记？」: the records or fields about to go have notes of the table's own.
pub(crate) fn ask(v: &TableView, window: &mut Window, cx: &Context<TableView>) -> Option<AnyElement> {
    let a = v.notes.ask.as_ref()?;
    let body = match &a.notes[..] {
        [one] => crate::localized_format!("同时把笔记「{}」移进回收站？撤销可以恢复。", "Also move the note “{}” to the Trash? Undo brings it back.", one.1),
        many => crate::localized_format!("同时把这 {} 篇笔记移进回收站？撤销可以恢复。", "Also move these {} notes to the Trash? Undo brings them back.", many.len()),
    };
    let (records, fields) = (a.records.clone(), a.fields.clone());
    let (r2, f2) = (records.clone(), fields.clone());
    let vp = window.viewport_size();
    let buttons = div().flex().items_center().justify_end().gap(px(8.))
        .child(crate::design::Button::outline("tb-note-ask-cancel").label(label("取消")).on_click(cx.listener(|v, _, _, cx| { v.notes.ask = None; cx.notify(); })))
        .child(crate::design::Button::outline("tb-note-ask-keep").label(label("保留笔记")).on_click(cx.listener(move |v, _, window, cx| finish(v, r2.clone(), f2.clone(), false, window, cx))))
        .child(crate::design::Button::destructive("tb-note-ask-all").label(label("一起删除")).on_click(cx.listener(move |v, _, window, cx| finish(v, records.clone(), fields.clone(), true, window, cx))));
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(
        crate::design::scrim().id("tb-note-ask-scrim").occlude().relative().w(vp.width).h(vp.height)
            .on_mouse_down(MouseButton::Left, cx.listener(|v, _, _, cx| { v.notes.ask = None; cx.notify(); }))
            .child(crate::design::Dialog::alert("tb-note-ask").w(px(420.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .title(label(if a.fields.is_empty() { "删除记录" } else { "删除字段/列" }))
                .description(body)
                .footer(buttons)))).with_priority(7).into_any_element())
}

#[cfg(test)]
mod tests {
    // (`test` here is GPUI's, through the glob)
    use super::*;

    fn note(id: &str) -> NoteInfo { NoteInfo { id: id.into(), title: String::new(), empty: true } }
    fn set(ids: &[&str]) -> HashSet<String> { ids.iter().map(|s| s.to_string()).collect() }

    /// A note this view made, then renamed in the sidebar: the cell still says the old path, and
    /// the note is known by its id all the same, so the undo of something else leaves it alone.
    #[core::prelude::v1::test]
    fn a_renamed_note_is_still_pointed_at() {
        let cell: CellId = (1, 2);
        let mut n = NotesUi { mine: HashMap::from([("a".to_string(), "书单/三体.xml".to_string())]), ids: HashMap::from([(cell, "a".to_string())]), ..Default::default() };
        n.index = Rc::new(HashMap::from([("书单/三体.xml".to_string(), note("a"))]));
        assert_eq!(target(&n, cell, Some("书单/三体.xml")), Some(&"a".to_string()));
        // renamed elsewhere: the library has it under its new name, the cell not yet
        n.index = Rc::new(HashMap::from([("书单/黑暗森林.xml".to_string(), note("a"))]));
        assert_eq!(target(&n, cell, Some("书单/三体.xml")), Some(&"a".to_string()));
        let at = set(&["a"]);
        assert_eq!(history_moves(&n, &at, &at), (vec![], vec![]));
        // in the 回收站 with its record, it is found by where it was
        n.index = Rc::new(HashMap::new());
        n.ids.clear();
        assert_eq!(target(&n, cell, Some("书单/三体.xml")), None);
        n.gone.insert("a".into());
        assert_eq!(target(&n, cell, Some("书单/三体.xml")), Some(&"a".to_string()));
    }

    /// Only the step that takes the last cell off a note sends it away, only the one that gives it one brings it back.
    #[core::prelude::v1::test]
    fn only_the_step_itself_moves_a_note() {
        let mut n = NotesUi { mine: HashMap::from([("a".to_string(), "a.xml".to_string())]), ..Default::default() };
        let (none, a) = (set(&[]), set(&["a"]));
        // the undo of the step that made it; of anything else, pointed at or (the cell emptied by hand) not
        assert_eq!(history_moves(&n, &a, &none), (vec![], vec!["a".to_string()]));
        assert_eq!(history_moves(&n, &a, &a), (vec![], vec![]));
        assert_eq!(history_moves(&n, &none, &none), (vec![], vec![]));
        // the undo of emptying the cell by hand: it never left
        assert_eq!(history_moves(&n, &none, &a), (vec![], vec![]));
        n.gone.insert("a".into());
        assert_eq!(history_moves(&n, &none, &a), (vec!["a".to_string()], vec![]));
        assert_eq!(history_moves(&n, &none, &none), (vec![], vec![]));
    }
}
