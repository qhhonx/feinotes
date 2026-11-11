//! Workspace window: two panes — the document list on the left
//! (see `sidebar.rs`) and the editor on the right, with the
//! 64px top bar (breadcrumb title + save status) above the page.

use crate::docinfo::DocInfo;
use crate::docmenu::DocSub;
use crate::editor::view::{Editor, EditorEvent};
use crate::input::{TextField, TextFieldEvent};
use crate::settings::display_title;
use crate::store::Entry;
use crate::shared_store::SharedStore;
use crate::theme::{self, Theme, craft};
use crate::ui::{TipExt, TipSide, TitleBarExt, ToastKind, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Autosave waits for this pause in editing, but never longer than
/// SAVE_MAX_DELAY after the oldest change not yet on disk.
const SAVE_DELAY: Duration = Duration::from_millis(500);
const SAVE_MAX_DELAY: Duration = Duration::from_secs(2);

actions!(workspace, [NewDoc, OpenFile, ImportDoc, ImportFolder, ToggleSidebar, FocusSearch, FindInDoc, ReplaceInDoc, OpenSettings, OpenAbout, CheckForUpdates, GoHome, CloseWindow, Quit,
    HideApp, HideOthers, ShowAll, Minimize, Zoom, ToggleFullScreen]);

/// The menu bar, in the interface language.
pub fn set_menus(cx: &mut App) {
    use crate::settings::tr;
    let name = crate::app_name();
    let mut app_items = vec![MenuItem::action(format!("{} {name}", tr("关于", "About")), OpenAbout)];
    if !crate::FEI_NOTES { app_items.push(MenuItem::action(tr("检查更新…", "Check for Updates…"), CheckForUpdates)); }
    app_items.extend([
        MenuItem::separator(),
        MenuItem::action(tr("设置…", "Settings…"), OpenSettings),
        MenuItem::separator(),
        MenuItem::os_submenu(tr("服务", "Services"), SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(format!("{} {name}", tr("隐藏", "Hide")), HideApp),
        MenuItem::action(tr("隐藏其他", "Hide Others"), HideOthers),
        MenuItem::action(tr("全部显示", "Show All"), ShowAll),
        MenuItem::separator(),
        MenuItem::action(format!("{} {name}", tr("退出", "Quit")), Quit),
    ]);
    cx.set_menus(vec![
        Menu { name: name.into(), items: app_items },
        Menu {
            name: tr("文件", "File").into(),
            items: vec![
                MenuItem::action(tr("新建文档", "New Document"), NewDoc),
                MenuItem::action(tr("新建笔记窗口", "New Note Window"), crate::quick_note::QuickNew),
                MenuItem::action(tr("打开 Markdown 文件…", "Open Markdown File…"), OpenFile),
                MenuItem::action(tr("导入为文档…", "Import as Document…"), ImportDoc),
                MenuItem::action(tr("导入文件夹…", "Import Folder…"), ImportFolder),
                MenuItem::separator(),
                MenuItem::action(tr("关闭窗口", "Close Window"), CloseWindow),
            ],
        },
        // The standard Edit menu: its items act on the focused editor, and
        // macOS adds 听写 and 表情与符号 to it.
        Menu {
            name: tr("编辑", "Edit").into(),
            items: {
                use crate::editor::keymap as k;
                vec![
                    MenuItem::os_action(tr("撤销", "Undo"), k::Undo, OsAction::Undo),
                    MenuItem::os_action(tr("重做", "Redo"), k::Redo, OsAction::Redo),
                    MenuItem::separator(),
                    MenuItem::os_action(tr("剪切", "Cut"), k::Cut, OsAction::Cut),
                    MenuItem::os_action(tr("拷贝", "Copy"), k::Copy, OsAction::Copy),
                    MenuItem::os_action(tr("粘贴", "Paste"), k::Paste, OsAction::Paste),
                    MenuItem::os_action(tr("全选", "Select All"), k::SelectAll, OsAction::SelectAll),
                ]
            },
        },
        Menu {
            name: tr("窗口", "Window").into(),
            items: vec![
                MenuItem::action(tr("最小化", "Minimize"), Minimize),
                MenuItem::action(tr("缩放", "Zoom"), Zoom),
                MenuItem::separator(),
                MenuItem::action(tr("进入 / 退出全屏幕", "Enter / Exit Full Screen"), ToggleFullScreen),
            ],
        },
    ]);
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-z", crate::editor::keymap::Undo, Some("Workspace")),
        KeyBinding::new("secondary-shift-z", crate::editor::keymap::Redo, Some("Workspace")),
        KeyBinding::new("secondary-y", crate::editor::keymap::Redo, Some("Workspace")),
        KeyBinding::new("cmd-n", NewDoc, None),
        KeyBinding::new("cmd-shift-n", crate::quick_note::QuickNew, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-shift-i", ImportDoc, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-alt-home", GoHome, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-]", ToggleSidebar, None),
        KeyBinding::new("cmd-\\", ToggleSidebar, None),
        KeyBinding::new("cmd-j", FocusSearch, None),
        KeyBinding::new("cmd-f", FindInDoc, None),
        KeyBinding::new("cmd-shift-h", ReplaceInDoc, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    // macOS app and window keys. ⌥⌘H (隐藏其他) stays with the editor's
    // 应用上次的颜色, the Feishu shortcut; 隐藏其他 is in the menu. On Windows
    // the system handles these (Win+↓, Alt+F4), and F11 is full screen.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
    ]);
    // Windows: F11 for full screen, and Ctrl+H for 查找和替换 as in Office
    // and Notepad (written cmd-h: `shortcuts::pc_binding` maps cmd to
    // ctrl and drops a literal ctrl-h, Cocoa's backspace).
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("f11", ToggleFullScreen, None), KeyBinding::new("cmd-h", ReplaceInDoc, None)]);
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(handle) = cx.active_window().and_then(|w| w.downcast::<Workspace>()) {
            // Key dispatch already borrows this window; close after it returns.
            cx.defer(move |cx| {
                if let Err(e) = handle.update(cx, |ws, window, cx| {
                    crate::quick_note::persist(cx);
                    if ws.save_before(cx, |_, window, _| window.remove_window()) { window.remove_window(); }
                }) { eprintln!("无法关闭笔记窗口：{e}"); }
            });
        }
    });
    cx.on_action(|_: &Quit, cx| cx.defer(quit));
    crate::modes::bind_keys(cx);
}

/// 退出: the open document is saved first; when that fails its dialog asks
/// (重试 / 放弃修改) and quits afterwards. Call it outside any window update
/// (`cx.defer`): a window on the update stack cannot be asked to save.
pub fn quit(cx: &mut App) {
    if save_windows_before(cx, quit) { cx.quit(); }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
pub(crate) fn restart(cx: &mut App) {
    use std::os::unix::process::CommandExt;
    if !save_windows_before(cx, restart) { return; }
    // Wait for the library lock to be released. A shared note can be open
    // in a separate instance of this bundle, so request a fresh instance.
    let result = cx.app_path().and_then(|app| {
        anyhow::ensure!(app.extension().is_some_and(|ext| ext == "app"), "Missing application bundle");
        // The offscreen harness needs its test environment after relaunch.
        let scripted_binary = if cfg!(feature = "devtools") && crate::devtools::scripted() {
            std::env::current_exe()?
        } else { std::path::PathBuf::new() };
        std::process::Command::new("/bin/sh").arg("-c").arg(r#"
            while kill -0 "$1" 2>/dev/null; do sleep 0.1; done
            if [ -n "$3" ]; then exec "$3"; fi
            exec /usr/bin/open -n "$2"
        "#).arg("sh").arg(std::process::id().to_string()).arg(app).arg(scripted_binary)
            .process_group(0).spawn()?;
        Ok(())
    });
    if let Err(error) = result {
        crate::logging::write(&format!("Could not restart after switching libraries: {error:#}"));
        for window in cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>()) {
            let _ = window.update(cx, |w, _, cx| {
                w.library_switching = false;
                w.show_toast_kind(ToastKind::Error, crate::settings::tr("无法自动重启。请退出后重新打开飞记。",
                    "Couldn’t restart automatically. Quit and reopen Fei Notes."), cx);
            });
        }
    } else { cx.quit(); }
}

pub(crate) fn save_windows_before(cx: &mut App, retry: fn(&mut App)) -> bool {
    cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Workspace>())
        .map(|w| w.update(cx, |ws, _, cx| ws.save_before(cx, move |_, _, cx| cx.defer(retry))).unwrap_or(false))
        .fold(true, |ready, saved| ready & saved)
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaveStatus {
    /// Freshly opened, nothing saved this session: show the modification time.
    Idle,
    Saved,
    Saving,
    Failed(String),
}

/// Right-click / "…" menu on a sidebar row.
#[derive(Clone, Debug)]
pub struct RowMenu {
    pub id: String,
    pub pos: Point<Pixels>,
    /// Which corner of the menu sits at `pos`.
    pub corner: Corner,
    /// 下载 submenu open.
    pub sub: bool,
    /// The 批量操作 menu for the checked rows.
    pub batch: bool,
}

/// What the main pane shows: the open document, 主页 or 回收站.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Doc,
    Home,
    Folder,
    Trash,
    Settings,
}

/// A confirmation dialog and the pages it is about.
#[derive(Debug)]
pub enum Confirm {
    /// 删除 (to the 回收站); `batch` for the 批量操作 selection.
    Delete { ids: Vec<String>, batch: bool },
    /// 回收站 → 恢复.
    Restore(String),
    /// 回收站 → 彻底删除.
    Purge(String),
    /// 回收站 → 清空回收站: every trashed page, permanently.
    EmptyTrash,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    DiscardOfflineRecovery(String),
    /// 隐藏引用列表 (with 不再提示) / 显示引用列表.
    Refs { show: bool, no_ask: bool },
    /// The open document could not be written before it was to be replaced
    /// (another document, 新建, 退出): 重试 / 放弃修改, then carry on.
    SaveFailed,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    StartCloudTrial,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    SwitchCloudLibrary { cloud: bool, copy_local: bool },
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    RestoreLibraryBackup(Box<crate::cloud_sync::backup::Prepared>),
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    CloudVersions { id: String, versions: Vec<(i64, String, String)>, selected: Option<i64> },
}

/// What to do once the open document is safely on disk (or its changes given up).
pub type AfterSave = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

/// Inline 重命名 of a sidebar row.
pub struct Rename {
    pub id: String,
    pub field: Entity<TextField>,
    _subs: Vec<Subscription>,
}

pub struct Workspace {
    pub quick_note: bool,
    pub shortcut_recording: Option<String>,
    pub shortcut_strokes: Vec<String>,
    /// The failed change: (action, message); an empty action is page-wide.
    pub shortcut_error: Option<(String, String)>,
    pub settings_page: crate::settings::Page,
    /// 外观's theme dropdown while open: the theme its preview shows.
    pub theme_menu: Option<String>,
    /// Where the theme dropdown's button was drawn last frame.
    pub theme_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// 快捷键's filter field.
    pub settings_filter: Entity<TextField>,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub sharing: crate::settings::Sharing,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub library_switching: bool,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub subscription: crate::settings::Subscription,
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub sync_issues: Option<Vec<crate::cloud_sync::SyncIssue>>,
    native_sidebar: crate::native_header::NativeSidebar,
    pub store: SharedStore,
    pub entries: Vec<Entry>,
    pub current: Option<String>,
    pub editor: Option<Entity<Editor>>,
    pub search: Entity<TextField>,
    /// Keyboard highlight inside search results.
    pub search_sel: Option<usize>,
    pub sidebar_open: bool,
    /// Dragged width (300 by default, 268 to min(800, window − 540)).
    pub sidebar_width: f32,
    /// The resize handle is being dragged.
    pub sidebar_drag: bool,
    pub folder_current: Option<String>,
    pub create_menu: Option<(Option<String>, Point<Pixels>)>,
    /// The header's 导入 menu: the folder it imports into, and where it hangs.
    pub import_menu: Option<(Option<String>, Point<Pixels>)>,
    pub folder_drag: crate::sidebar_drag::SidebarDrag,
    /// The search dialog (command-center style) is open.
    pub search_open: bool,
    pub status: SaveStatus,
    pub row_menu: Option<RowMenu>,
    pub rename: Option<Rename>,
    pub(crate) file_history: crate::file_history::FileHistory,
    /// Last-frame bounds of each row's "…" button, to hang its menu from.
    pub more_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    /// Header "…" menu and its open submenu.
    pub doc_menu: bool,
    pub doc_sub: Option<DocSub>,
    /// Last-frame bounds of the open submenu (clicks there keep the menu open).
    pub doc_sub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub doc_info: Option<DocInfo>,
    pub list_scroll: ScrollHandle,
    pub search_scroll: ScrollHandle,
    /// Last-frame bounds of the row menu's 下载 submenu.
    pub row_sub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// 批量操作 mode: the checked docs of 我的文档, in check order.
    pub batch: Option<Vec<String>>,
    /// The open confirmation dialog.
    pub confirm: Option<Confirm>,
    pub view: View,
    pub home: crate::home::Home,
    pub settings_scroll: ScrollHandle,
    /// 回收站 rows, refreshed when the page opens or changes.
    pub trash: Vec<crate::store::TrashEntry>,
    /// A 回收站 row's menu: the page and where it opens.
    pub trash_menu: Option<(String, Point<Pixels>, Corner)>,
    /// The toast and when it appeared.
    pub toast: Option<(ToastKind, SharedString, Instant)>,
    toast_task: Option<Task<()>>,
    /// Folded sidebar sections: 置顶, 我的文档.
    pub folded: [bool; 3],
    /// Expanded pages of the 我的文档 tree.
    pub expanded: HashSet<String>,
    pub move_dlg: Option<crate::tree::MoveDialog>,
    /// 导出 Markdown 设置.
    pub export_md: Option<crate::transfer::ExportMd>,
    /// 编辑 / 阅读 dropdown, and the mode button's last-frame bounds.
    pub mode_menu: bool,
    pub mode_btn_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The header "…" button, which the doc menu hangs from.
    pub more_btn_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The 快捷键 drawer, and the help button's menu (open while hovered).
    pub shortcuts: bool,
    pub(crate) focus: FocusHandle,
    save_task: Option<Task<()>>,
    /// When the oldest edit not yet on disk was made: a save waits for a
    /// pause in typing, but never longer than `SAVE_MAX_DELAY` after it.
    dirty_since: Option<Instant>,
    /// The open document has changes not yet on disk (cleared only by a
    /// successful save or by 放弃修改).
    unsaved: bool,
    /// Held back by a failed save: runs after 重试 succeeds or 放弃修改.
    pub(crate) after_save: Option<AfterSave>,
    pub(crate) editor_sub: Option<Subscription>,
    _subs: Vec<Subscription>,
}


/// A macOS notification (only logged when scripted).
fn notify_system(title: &str, body: &str) {
    if crate::devtools::scripted() {
        return eprintln!("notification: {title} / {body}");
    }
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", q(body), q(title));
    std::thread::spawn(move || std::process::Command::new("osascript").arg("-e").arg(script).status());
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_window(false, window, cx)
    }

    pub fn new_window(quick: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window(quick, None, window, cx)
    }

    pub fn new_document_window(document: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window(true, document, window, cx)
    }

    fn build_window(quick: bool, document: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::settings::load();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cx.observe_global_in::<crate::cloud_subscription::PurchaseChanged>(window, |ws, window, cx| {
            ws.subscription_updated(window, cx);
        }).detach();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::SyncStatus>(|ws, cx| {
            if ws.view == View::Settings {
                if ws.sync_issues.is_some() {
                    let status = cx.global::<crate::cloud_sync::SyncStatus>();
                    if status.held == 0 && status.attachments == 0 { ws.sync_issues = Some(vec![]); }
                    else if status.uploads == 0 && status.downloads == 0
                        && let Ok(issues) = crate::cloud_sync::issues(cx) { ws.sync_issues = Some(issues); }
                }
                cx.notify();
            }
        }).detach();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::activity::ActivityView>(|_, cx| cx.notify()).detach();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::presence::View>(|ws, cx| {
            let remote = ws.current.as_ref().and_then(|id| cx.global::<crate::cloud_sync::presence::View>().notes.get(id))
                .cloned().unwrap_or_default();
            if let Some(editor) = ws.editor.clone() {
                editor.update(cx, |editor, cx| {
                    if editor.remote_selections != remote {
                        editor.remote_selections = remote;
                        cx.notify();
                    }
                });
            }
            if ws.view == View::Settings || crate::in_shared_note() { cx.notify(); }
        }).detach();
        if !cx.has_global::<SharedStore>() { cx.set_global(SharedStore::open().expect("open document library")); }
        let mut store = cx.global::<SharedStore>().clone();
        let entries = store.list();
        let search = cx.new(|cx| {
            let mut f = TextField::new("搜索", cx);
            f.placeholder_color = Theme::text_disabled();
            f
        });
        let settings_filter = cx.new(|cx| {
            let mut f = TextField::new(crate::settings::tr("搜索快捷键", "Search shortcuts"), cx);
            f.placeholder_color = Theme::text_disabled();
            f
        });
        let subs = vec![
            cx.subscribe(&settings_filter, |_, _, ev: &TextFieldEvent, cx| if matches!(ev, TextFieldEvent::Changed) { cx.notify(); }),
            cx.observe_window_bounds(window, |this, window, cx| crate::quick_note::remember(window, this.quick_note, cx)),
            // The system switched between light and dark (or the app's own look changed).
            cx.observe_window_appearance(window, |this, _, cx| if crate::settings::sync_dark(cx) { crate::settings::refresh_all(this, cx); }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() { if let Some(id) = &this.current { crate::quick_note::used(id, cx); } this.entries = this.store.list(); this.sort(); crate::update::check_if_stale(cx); cx.notify(); }
                else { this.cancel_home_drag(cx); this.end_folder_drag(true, cx); this.cancel_shortcut_recording(cx); this.flush(cx); }
            }),
            cx.subscribe_in(&search, window, |this, _, ev: &TextFieldEvent, window, cx| {
                match ev {
                    TextFieldEvent::Changed => {
                        this.search_sel = this.default_search_sel(cx);
                        this.search_scroll.set_offset(point(px(0.), px(0.)));
                    }
                    TextFieldEvent::Up => this.move_search_sel(-1, cx),
                    TextFieldEvent::Down => this.move_search_sel(1, cx),
                    TextFieldEvent::Submit => {
                        if let Some(ix) = this.search_sel {
                            this.pick_search(ix, window, cx)
                        }
                    }
                    TextFieldEvent::Cancel => this.close_search(window, cx),
                    _ => {}
                }
                cx.notify();
            }),
            // Last resort (the system logging out): ⌘Q and the close button
            // go through `save_before` and ask when the save fails.
            cx.on_app_quit(|this, cx| {
                crate::quick_note::persist(cx);
                if !this.flush(cx) && let Some(ed) = &this.editor
                    && let Err(error) = this.store.recover_later(&ed.read(cx).core.doc) {
                    crate::logging::write(&format!("Could not preserve unsaved document: {error:#}"));
                }
                async {}
            }),
        ];
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            crate::quick_note::persist(cx);
            this.update(cx, |w, cx| { w.cancel_shortcut_recording(cx); w.save_before(cx, |_, window, _| window.remove_window()) }).unwrap_or(true)
        });
        if !quick { crate::settings::watch_themes(cx); }
        // 日期提醒: post what fell due since the last look (not what was
        // missed while the app was closed).
        cx.spawn(async |this, cx| {
            let mut last = crate::time::now_ms() as i64;
            loop {
                cx.background_executor().timer(Duration::from_secs(15)).await;
                let now = crate::time::now_ms() as i64;
                if this.update(cx, |w, cx| w.check_reminders(last, now, cx)).is_err() {
                    break;
                }
                last = now;
            }
        })
        .detach();
        let sidebar_width = store.settings().sidebar_width.unwrap_or(theme::SIDEBAR_WIDTH);
        let mut ws = Workspace {
            quick_note: quick,
            shortcut_recording: None,
            shortcut_strokes: vec![],
            shortcut_error: None,
            settings_page: Default::default(),
            theme_menu: None,
            theme_anchor: Default::default(),
            settings_filter,
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            sharing: crate::settings::Sharing::new(cx),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            library_switching: false,
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            subscription: Default::default(),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            sync_issues: None,
            store,
            entries,
            current: None,
            editor: None,
            search,
            search_sel: None,
            sidebar_open: !quick,
            sidebar_width,
            sidebar_drag: false,
            folder_current: None,
            create_menu: None,
            import_menu: None,
            folder_drag: Default::default(),
            search_open: false,
            status: SaveStatus::Idle,
            row_menu: None,
            rename: None,
            file_history: Default::default(),
            more_bounds: Default::default(),
            doc_menu: false,
            doc_sub: None,
            doc_sub_bounds: Rc::new(Cell::new(None)),
            doc_info: None,
            list_scroll: ScrollHandle::new(),
            search_scroll: ScrollHandle::new(),
            row_sub_bounds: Rc::new(Cell::new(None)),
            batch: None,
            confirm: None,
            view: View::Doc,
            home: Default::default(),
            settings_scroll: ScrollHandle::new(),
            trash: vec![],
            trash_menu: None,
            toast: None,
            toast_task: None,
            mode_menu: false,
            mode_btn_bounds: Rc::new(Cell::new(None)),
            more_btn_bounds: Rc::new(Cell::new(None)),
            native_sidebar: Default::default(),
            shortcuts: false,
            folded: [false; 3],
            expanded: HashSet::new(),
            move_dlg: None,
            export_md: None,
            focus: cx.focus_handle(),
            save_task: None,
            dirty_since: None,
            unsaved: false,
            after_save: None,
            editor_sub: None,
            _subs: subs,
        };
        ws.sort();
        ws.load_home();
        // An empty library opens on 主页's empty state; only a quick note
        // window starts a document of its own.
        match document.or_else(|| (!quick).then(|| ws.entries.first().map(|e| e.id.clone())).flatten()) {
            Some(id) => {
                if !ws.try_open(&id, window, cx) && crate::in_shared_note() { ws.view = View::Home; }
            }
            None if quick => ws.new_doc(window, cx),
            None => ws.view = View::Home,
        }
        // Kept in the top bar (not just a toast): the user should notice.
        let warnings = ws.store.take_warnings();
        if !warnings.is_empty() {
            let msg = warnings.iter().map(|s| crate::settings::diagnostic(s)).collect::<Vec<_>>().join(crate::settings::tr("；", "; "));
            ws.show_toast_kind(ToastKind::Error, &msg, cx);
            ws.status = SaveStatus::Failed(msg);
        }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if !quick && warnings.is_empty() {
            use crate::cloud_restore::{BACKUP_RESTORED, BACKUP_RESTORE_ERROR};
            if let Some(error) = BACKUP_RESTORE_ERROR.get() {
                ws.show_toast_kind(ToastKind::Error, &crate::localized_format!(
                    "备份尚未恢复 · {}", "Backup restore incomplete · {}", crate::settings::diagnostic(error)), cx);
            } else if BACKUP_RESTORED.get().is_some() {
                ws.show_toast(crate::settings::tr("已恢复备份，之前的文档库已保留。",
                    "Backup restored. Your previous library was kept."), cx);
            } else if let Some(reason) = crate::cloud_restore::startup_fallback() {
                ws.show_toast(reason, cx);
            }
        }
        ws
    }

    /// Most recently modified first.
    pub(crate) fn sort(&mut self) {
        self.reapply_pending_tree_moves();
        self.entries.sort_by(|a, b| b.meta.modified.cmp(&a.meta.modified).then_with(|| a.id.cmp(&b.id)));
    }

    pub fn query(&self, cx: &App) -> String {
        self.search.read(cx).text.trim().to_lowercase()
    }

    pub fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.try_open(id, window, cx);
    }

    /// Show `id`; false when it could not be opened (or the open document
    /// could not be saved first, see [`Self::save_before`]).
    pub fn try_open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let Some(room) = crate::cloud_sync::shared::active()
            && (id != room.note || !self.entries.iter().any(|entry| entry.id == id)
                || crate::cloud_sync::shared::access(cx) == Some(crate::cloud_sync::shared::Access::Revoked))
        { return false; }
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) { return self.open_folder(id, window, cx); }
        if self.view != View::Doc {
            self.view = View::Doc;
            cx.notify();
        }
        if self.current.as_deref() == Some(id) {
            return true;
        }
        let target = id.to_string();
        if !self.save_before(cx, move |w, window, cx| {
            w.open(&target, window, cx);
        }) {
            return false;
        }
        let shared = !self.document_peers(id, window, cx).is_empty();
        let doc = match self.store.load_for_edit(id, shared) {
            Ok(d) => d,
            Err(e) => {
                self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文档：{}", "Could not open document: {}", crate::settings::diagnostic(&e.to_string())), cx);
                cx.notify();
                return false;
            }
        };
        let meta = self.entries.iter().find(|e| e.id == id).map(|e| e.meta.clone());
        let modified = meta.as_ref().map_or_else(crate::time::now, |m| m.modified);
        let width = meta.as_ref().map(|m| m.width).unwrap_or_default();
        let pinned = meta.as_ref().is_some_and(|m| !m.catalogue_closed);
        let editor = cx.new(|cx| {
            let mut e = Editor::new(doc, modified, width, window, cx);
            e.cat_pinned = pinned;
            e.markdown = meta.as_ref().is_some_and(|m| m.markdown);
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            {
                e.cloud_history = !crate::cloud_restore::local_only() && meta.as_ref().is_some_and(|m| !m.local());
                e.remote_selections = cx.try_global::<crate::cloud_sync::presence::View>()
                    .and_then(|view| view.notes.get(id)).cloned().unwrap_or_default();
            }
            if let Some(m) = &meta {
                e.created = m.created;
                e.info = m.info;
                e.set_read_only(m.reading, cx);
            }
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            if crate::cloud_sync::shared::editing_paused(cx)
                || crate::cloud_sync::shared::access(cx).is_some_and(|access| !access.can_write()) {
                e.set_read_only(true, cx);
            }
            e
        });
        // Reuse the live state, including unsaved edits and undo history.
        if let Some(peer) = self.document_peers(id, window, cx).first() {
            let core = peer.read(cx).core.clone();
            editor.update(cx, |e, cx| e.receive_document(&core, cx));
        }
        self.editor_sub = Some(cx.subscribe_in(&editor, window, |this, _, ev: &EditorEvent, window, cx| match ev {
            EditorEvent::Changed => {
                this.file_history.prefer_files = false;
                // 阅读模式 put the document back: nothing to save.
                let rejected = this.editor.clone().is_some_and(|ed| ed.update(cx, |e, cx| e.reject_read_only_edit(cx)));
                if !rejected {
                    this.sync_document(window, cx);
                    this.schedule_save(window, cx);
                }
            }
            EditorEvent::Mode(reading) => this.set_mode(*reading, window, cx),
            EditorEvent::Info(info) => this.set_info_row(*info),
            EditorEvent::Toast => {
                this.toast = None;
                cx.notify();
            }
            EditorEvent::Save => {
                this.flush(cx);
            }
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            EditorEvent::CloudUndo { redo } => {
                if this.flush(cx)
                    && let (Some(id), Some(editor)) = (this.current.clone(), this.editor.clone())
                {
                    let core = editor.read(cx).core.clone();
                    match crate::cloud_sync::undo(&mut this.store, &id, &core, *redo, cx) {
                        Ok(Some(change)) => cx.defer(move |cx| cloud_notes_changed(&[change], false, cx)),
                        Ok(None) => {}
                        Err(error) => this.show_toast(&crate::localized_format!("无法更新撤销历史：{}", "Could not update undo history: {}", error), cx),
                    }
                }
            }
            EditorEvent::Search(q) => {
                this.open_search(window, cx);
                this.search.update(cx, |s, cx| s.set_text(q, cx));
                this.search_sel = this.default_search_sel(cx);
            }
            EditorEvent::OpenDoc(id) => this.open(id, window, cx),
            EditorEvent::OpenDocAt(id, ordinal) => this.open_at(id, *ordinal, window, cx),
            EditorEvent::HideRefs => this.ask_refs(false, cx),
            EditorEvent::Duplicate => {
                if let Some(id) = this.current.clone() {
                    this.duplicate_doc(&id, window, cx);
                }
            }
            EditorEvent::Catalogue(pinned) => {
                if let Some(id) = this.current.clone() {
                    if let Some(e) = this.entries.iter_mut().find(|e| e.id == id) {
                        e.meta.catalogue_closed = !pinned;
                    }
                    let _ = this.store.set_catalogue(&id, *pinned);
                }
            }
        }));
        // Opening a document puts the caret in it, unless the user is typing a search.
        if !self.search_open {
            editor.read(cx).focus(window);
        }
        if let Some(m) = self.store.touch(id)
            && let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                e.meta.opened = m.opened;
            }
        if let Some(previous) = &self.current
            && self.document_peers(previous, window, cx).is_empty() { self.store.forget_loaded(previous); }
        self.editor = Some(editor);
        self.current = Some(id.to_string());
        crate::quick_note::used(id, cx);
        self.reveal_in_tree(id);
        self.sync_sub_pages(cx);
        self.status = SaveStatus::Idle;
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        self.mode_menu = false;
        cx.notify();
        true
    }

    pub fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::in_shared_note() { return; }
        if !self.save_before(cx, |w, window, cx| w.new_doc(window, cx)) {
            return;
        }
        let file_before = self.store.track_file_changes();
        let parent = self.creation_parent();
        match self.store.create().and_then(|(id, doc)| {
            if let Some(parent) = &parent { self.store.set_parent(&id, Some(parent))?; }
            Ok((id, doc))
        }) {
            Ok((id, _)) => {
                self.search_open = false;
                self.doc_info = None;
                self.home.sort_menu = false;
                self.entries = self.store.list();
                self.sort();
                self.search.update(cx, |s, cx| s.set_text("", cx));
                self.open(&id, window, cx);
                if let Some(ed) = &self.editor {
                    ed.read(cx).focus(window);
                }
            }
            Err(e) => self.status = SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.remember_files(file_before, crate::settings::tr("新建文档", "New document"));
        cx.notify();
    }

    /// Ask before 删除: a confirmation, then the 回收站.
    pub fn ask_delete(&mut self, id: &str, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        self.confirm = Some(Confirm::Delete { ids: vec![id.to_string()], batch: false });
        cx.notify();
    }

    /// Move documents to the trash; if the open one goes, open the next.
    pub fn delete_docs(&mut self, ids: &[String], window: &mut Window, cx: &mut Context<Self>) {
        // A page goes with its child pages: each topmost page is
        // trashed as one item that carries its subtree.
        let tree = crate::tree::Tree::new(&self.entries);
        let mut all: Vec<(String, Option<String>)> = vec![];
        for id in ids {
            let covered = self.ancestors(id).iter().any(|a| ids.contains(&a.id));
            if covered {
                continue;
            }
            for d in tree.subtree(id) {
                if !all.iter().any(|(x, _)| *x == d) {
                    let root = (d != *id).then(|| id.clone());
                    all.push((d, root));
                }
            }
        }
        let groups = all;
        let all: Vec<String> = groups.iter().map(|(d, _)| d.clone()).collect();
        for handle in cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id()) {
            if let Some(handle) = handle.downcast::<Workspace>()
                && handle.update(cx, |ws, _, _| ws.current.as_ref().is_some_and(|id| all.contains(id))).unwrap_or(false)
            {
                self.show_toast_kind(ToastKind::Error, crate::settings::text("请先关闭正在编辑这些文档的其他窗口，再删除"), cx);
                return;
            }
        }
        if self.current.as_ref().is_some_and(|c| all.contains(c)) {
            let retry = ids.to_vec();
            if !self.save_before(cx, move |w, window, cx| w.delete_docs(&retry, window, cx)) {
                return;
            }
        }
        let file_before = self.store.track_file_changes();
        let mut deleted = vec![];
        let mut errors = vec![];
        for (id, root) in &groups {
            // Do not split a subtree when its root could not be deleted.
            if root.as_ref().is_some_and(|r| !deleted.contains(r)) { continue; }
            match self.store.delete(id, root.as_deref()) {
                Ok(()) => deleted.push(id.clone()),
                Err(e) => errors.push(crate::localized_format!("删除失败：{}", "Could not delete document: {}", crate::settings::diagnostic(&e.to_string()))),
            }
        }
        if self.current.as_ref().is_some_and(|c| deleted.contains(c)) {
            self.save_task = None;
            self.unsaved = false;
            self.current = None;
            self.editor = None;
            self.editor_sub = None;
        }
        self.entries.retain(|e| !deleted.contains(&e.id));
        if self.folder_current.as_ref().is_some_and(|id| deleted.contains(id)) {
            self.folder_current = None;
            if self.view == View::Folder { self.view = View::Home; }
        }
        if let Some(b) = self.batch.as_mut() {
            b.retain(|id| !deleted.contains(id));
        }
        self.row_menu = None;
        if self.current.is_none() && self.view != View::Folder {
            // Stay on 主页 / 回收站 if that's where the delete came from.
            // The last document gone leaves the library empty (主页's empty
            // state), not a new 未命名 document.
            let view = self.view;
            match self.entries.first().map(|e| e.id.clone()) {
                Some(next) => { self.open(&next, window, cx); self.view = view; }
                None => self.view = if view == View::Doc { View::Home } else { view },
            }
        }
        self.remember_files(file_before, crate::settings::tr("删除", "Delete"));
        self.sync_sub_pages(cx);
        if self.view == View::Trash {
            self.trash = self.store.list_trash();
        }
        errors.extend(self.store.take_warnings().iter().map(|s| crate::settings::diagnostic(s)));
        if errors.is_empty() {
            self.show_toast(crate::settings::text("删除成功"), cx);
        } else {
            let msg = errors.join("；");
            self.show_toast_kind(ToastKind::Error, &msg, cx);
            self.status = SaveStatus::Failed(msg);
        }
        cx.notify();
    }

    pub fn toggle_pin(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut on = false;
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.meta.pinned = !e.meta.pinned;
            on = e.meta.pinned;
            let _ = self.store.set_pinned(id, e.meta.pinned);
        }
        self.sort();
        self.row_menu = None;
        self.show_toast(if on { crate::settings::text("已添加到“置顶”") } else { crate::settings::text("已从“置顶”移除") }, cx);
        cx.notify();
    }

    /// 收藏 / 取消收藏 (listed under 主页 → 收藏).
    pub fn toggle_star(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(e) = self.entries.iter_mut().find(|e| e.id == id) else { return };
        let on = e.meta.starred == 0;
        if let Ok(t) = self.store.set_starred(id, on) {
            e.meta.starred = t;
        }
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        self.show_toast(if on { crate::settings::text("已添加到“收藏”") } else { crate::settings::text("已从“收藏”中移除") }, cx);
        cx.notify();
    }

    /// Switch the main pane to 主页 or 回收站 (or back to the document).
    pub fn show_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.cancel_shortcut_recording(cx);
        if view == View::Trash {
            self.trash = self.store.list_trash();
        }
        self.view = view;
        self.row_menu = None;
        self.trash_menu = None;
        self.theme_menu = None;
        self.home.sort_menu = false;
        cx.notify();
    }

    /// A success toast (删除成功, 移动成功 …).
    pub fn show_toast(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.show_toast_kind(ToastKind::Success, msg, cx);
    }

    pub fn show_toast_kind(&mut self, kind: ToastKind, msg: &str, cx: &mut Context<Self>) {
        self.toast = Some((kind, SharedString::from(msg.to_string()), Instant::now()));
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| e.clear_toast(cx));
        }
        self.toast_task = Some(cx.spawn(async |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2700)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
        cx.notify();
    }

    fn render_toast(&self, window: &mut Window) -> Option<AnyElement> {
        let (kind, msg, t0) = self.toast.as_ref()?;
        crate::ui::toast_el(*kind, msg, *t0, window)
    }

    /// 重命名: the row's title turns into an input; Enter or clicking away saves, Esc cancels.
    pub fn start_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self.entries.iter().find(|e| e.id == id).map(|e| e.title.trim().to_string()) else { return };
        let field = cx.new(|cx| {
            let mut f = TextField::new(crate::model::UNTITLED, cx);
            f.set_text(&title, cx);
            f.select_all_text(cx);
            f
        });
        let focus = field.focus_handle(cx);
        window.focus(&focus);
        let subs = vec![
            cx.subscribe_in(&field, window, |w, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submit => w.finish_rename(true, window, cx),
                TextFieldEvent::Cancel => w.finish_rename(false, window, cx),
                _ => {}
            }),
            cx.on_blur(&focus, window, |w, window, cx| w.finish_rename(true, window, cx)),
        ];
        self.rename = Some(Rename { id: id.to_string(), field, _subs: subs });
        // From 主页 the row may be hidden: show it in the sidebar.
        self.sidebar_open = true;
        self.reveal_in_tree(id);
        self.row_menu = None;
        cx.notify();
    }

    fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.rename.take() else { return };
        let refocus = r.field.read(cx).is_focused(window);
        let title = r.field.read(cx).text.trim().to_string();
        let old = self.entries.iter().find(|e| e.id == r.id).map(|e| e.title.trim().to_string());
        if commit && old.is_some_and(|o| o != title) {
            if let Some(peer) = self.document_peers(&r.id, window, cx).first() {
                peer.update(cx, |e, cx| e.set_title(&title, cx));
            } else {
                if !self.flush(cx) { return; }
                let file_before = self.store.track_file_changes();
                match self.store.rename(&r.id, &title) {
                    Ok(meta) => {
                        if let Some(e) = self.entries.iter_mut().find(|e| e.id == r.id) {
                            e.title = title.clone();
                            e.meta = meta;
                        }
                        self.sort();
                        if self.current.as_deref() == Some(r.id.as_str()) && let Some(ed) = &self.editor {
                            ed.update(cx, |e, cx| e.receive_file_title(&title, cx));
                        }
                        self.remember_files(file_before, crate::settings::tr("重命名", "Rename"));
                    }
                    Err(e) => self.status = SaveStatus::Failed(crate::localized_format!("重命名失败：{}", "Could not rename document: {}", crate::settings::diagnostic(&e.to_string()))),
                }
            }
        }
        if refocus
            && let Some(ed) = &self.editor {
                ed.read(cx).focus(window);
            }
        self.sync_sub_pages(cx);
        cx.notify();
    }

    pub(crate) fn document_peers(&self, id: &str, window: &Window, cx: &mut Context<Self>) -> Vec<Entity<Editor>> {
        cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id())
            .filter_map(|w| w.downcast::<Workspace>())
            .filter_map(|w| w.update(cx, |ws, _, _| {
                (ws.current.as_deref() == Some(id)).then(|| ws.editor.clone()).flatten()
            }).ok().flatten()).collect()
    }

    fn sync_document(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (Some(id), Some(editor)) = (self.current.clone(), self.editor.clone()) else { return };
        let peers = self.document_peers(&id, window, cx);
        if peers.is_empty() { return; }
        let core = editor.read(cx).core.clone();
        for peer in peers { peer.update(cx, |e, cx| e.receive_document(&core, cx)); }
        for handle in cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id()).filter_map(|w| w.downcast::<Workspace>()) {
            let _ = handle.update(cx, |ws, _, cx| {
                if let Some(entry) = ws.entries.iter_mut().find(|e| e.id == id) {
                    entry.title = core.doc.title.text.clone();
                    entry.icon = core.doc.icon.clone();
                }
                // Every remaining view can save the latest state if its sibling closes.
                if ws.current.as_deref() == Some(&id) { ws.unsaved = true; }
                cx.notify();
            });
        }
    }

    fn schedule_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.status = SaveStatus::Saving;
        if !self.unsaved || self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
        self.unsaved = true;
        // Each edit replaces the timer (dropping a Task cancels it), so steady
        // typing would put the save off for as long as it lasts: the deadline
        // caps that at SAVE_MAX_DELAY after the oldest unsaved edit.
        let left = SAVE_MAX_DELAY.saturating_sub(self.dirty_since.map_or(Duration::ZERO, |t| t.elapsed()));
        let delay = SAVE_DELAY.min(left);
        // Keep the list title live while typing; the body/snippet updates on save.
        if let (Some(id), Some(ed)) = (&self.current, &self.editor) {
            let title = ed.read(cx).core.doc.title.text.clone();
            if let Some(e) = self.entries.iter_mut().find(|e| &e.id == id) {
                e.title = title;
            }
        }
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            loop {
                match this.update(cx, |w, cx| w.editor.as_ref().is_some_and(|ed| ed.read(cx).composing())) {
                    Ok(true) => cx.background_executor().timer(SAVE_DELAY).await,
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            let _ = this.update(cx, |w, cx| w.flush(cx));
        }));
        cx.notify();
    }

    fn check_reminders(&mut self, from: i64, to: i64, cx: &mut Context<Self>) {
        // The open document's reminders are read live (unsaved edits too).
        let open = self.editor.as_ref().map(|e| e.read(cx).core.doc.reminders());
        for e in &self.entries {
            let rs = match &open {
                Some(rs) if self.current.as_deref() == Some(e.id.as_str()) => rs,
                _ => &e.reminders,
            };
            for r in rs.iter().filter(|r| r.should_notify && from < r.notify && r.notify <= to) {
                notify_system(&display_title(&e.title), &crate::editor::reminder::notification(r));
            }
        }
    }

    /// Write the open document to disk if it has unsaved changes. False when
    /// that failed: the changes stay pending (a later save retries them) and
    /// the top bar shows the error.
    pub fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        self.save_task = None;
        if !self.unsaved {
            return true;
        }
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else {
            self.unsaved = false;
            return true;
        };
        let doc = &ed.read(cx).core.doc;
        let (title, icon, body, reminders, links) = (doc.title.text.clone(), doc.icon.clone(), doc.plain_text(), doc.reminders(), doc.doc_links());
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let core = ed.read(cx).core.clone();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let result = crate::cloud_sync::save(&mut self.store, &id, &core, cx);
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let result = self.store.save(&id, doc).map(|meta| (meta, false));
        match result {
            Ok((meta, held)) => {
                if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                    e.title = title;
                    e.icon = icon;
                    e.body = body.into();
                    e.reminders = reminders;
                    e.links = links;
                    e.meta = meta.clone();
                }
                self.sync_links(cx);
                ed.update(cx, |e, cx| {
                    e.modified = meta.modified;
                    cx.notify();
                });
                self.status = SaveStatus::Saved;
                self.unsaved = false;
                self.sort();
                #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                if cx.windows().len() > 1 {
                    cx.defer(move |cx| cloud_saved_mirrors(&id, &core.doc, cx));
                }
                if held {
                    self.show_toast(crate::settings::tr("已保存在本机，部分内容无法同步。请查看文档库设置。",
                        "Saved on this Mac. See Documents settings for items that could not sync."), cx);
                }
            }
            Err(e) => self.status = SaveStatus::Failed(crate::localized_format!("保存失败：{}", "Could not save: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        // A failed save tries again SAVE_MAX_DELAY after this attempt, not on every keystroke.
        self.dirty_since = self.unsaved.then(Instant::now);
        cx.notify();
        !self.unsaved
    }

    /// Save the open document before something replaces it. When the save
    /// fails nothing is replaced: the 保存失败 dialog offers 重试 / 放弃修改
    /// and runs `then` after either.
    pub fn save_before(&mut self, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> bool {
        if self.flush(cx) {
            return true;
        }
        self.after_save = Some(Box::new(then));
        self.confirm = Some(Confirm::SaveFailed);
        cx.notify();
        false
    }

    /// The 保存失败 dialog's choices: `retry` saves again, otherwise the
    /// changes are given up. Either way the held-back action runs once the
    /// document no longer needs saving.
    pub(crate) fn resolve_save_failed(&mut self, retry: bool, window: &mut Window, cx: &mut Context<Self>) {
        if retry {
            if !self.flush(cx) {
                self.confirm = Some(Confirm::SaveFailed);
                return;
            }
        } else {
            let Some(id) = self.current.clone() else { return };
            match self.store.load_for_edit(&id, false) {
                Ok(doc) => {
                    if let Some(ed) = &self.editor {
                        ed.update(cx, |e, cx| e.receive_document(&crate::editor::core::Core::new(doc), cx));
                    }
                    self.sync_document(window, cx);
                    self.save_task = None;
                    self.unsaved = false;
                    self.status = SaveStatus::Idle;
                    self.dirty_since = None;
                    self.entries = self.store.list();
                    self.sort();
                }
                Err(e) => {
                    self.status = SaveStatus::Failed(e.to_string());
                    self.confirm = Some(Confirm::SaveFailed);
                    return;
                }
            }
        }
        if let Some(then) = self.after_save.take() {
            then(self, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn save_failed_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ed) = &self.editor else { return };
        let mut doc = ed.read(cx).core.doc.clone();
        doc.title = crate::model::RichText::plain(format!("{}{}", doc.title.text, crate::settings::tr("（恢复副本）", " (recovered copy)")));
        match self.store.create_from(&doc) {
            Ok(_) => {
                self.entries = self.store.list();
                self.sort();
                self.confirm = None;
                self.resolve_save_failed(false, window, cx);
            }
            Err(e) => {
                self.status = SaveStatus::Failed(e.to_string());
                self.confirm = Some(Confirm::SaveFailed);
                cx.notify();
            }
        }
    }

    /// The error of the last failed save, for the 保存失败 dialog.
    pub(crate) fn save_error(&self) -> String {
        match &self.status {
            SaveStatus::Failed(e) => e.clone(),
            _ => crate::settings::text("保存失败").to_string(),
        }
    }

    /// Hand the open document the library's titles and the documents
    /// linking to it (双向链接).
    pub(crate) fn sync_links(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else { return };
        let docs = self
            .entries
            .iter()
            .map(|e| crate::editor::links::DocRef { id: e.id.clone(), title: e.title.clone(), icon: e.icon.clone(), modified: e.meta.modified, opened: e.meta.opened })
            .collect();
        let backlinks = self
            .entries
            .iter()
            .filter(|e| e.id != id)
            .filter_map(|e| {
                let lines: Vec<_> = e.links.iter().filter(|l| l.target == id).cloned().collect();
                (!lines.is_empty()).then(|| crate::editor::links::Backlink { id: e.id.clone(), lines })
            })
            .collect();
        let hidden = self.entries.iter().find(|e| e.id == id).is_some_and(|e| e.meta.refs_hidden);
        ed.update(cx, |e, cx| e.set_links(id, docs, backlinks, hidden, cx));
    }

    /// A 反向引用 line: open that document at its `ordinal`th block.
    fn open_at(&mut self, id: &str, ordinal: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.try_open(id, window, cx) {
            return;
        }
        let Some(ed) = self.editor.clone() else { return };
        ed.update(cx, |e, cx| {
            let mut n = 0;
            let mut target = None;
            e.core.doc.for_each_block(|b| {
                if n == ordinal {
                    target = Some(b.id);
                }
                n += 1;
            });
            if let Some(b) = target {
                e.scroll_to_block(b, cx);
            }
        });
    }

    /// 隐藏引用列表 / 显示引用列表: confirm first (hiding can skip it with 不再提示).
    pub(crate) fn ask_refs(&mut self, show: bool, cx: &mut Context<Self>) {
        if !show && self.store.settings().refs_hide_no_ask {
            return self.set_refs_hidden(true, cx);
        }
        self.confirm = Some(Confirm::Refs { show, no_ask: false });
        cx.notify();
    }

    pub(crate) fn set_refs_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        let Some(id) = self.current.clone() else { return };
        if self.store.update_meta(&id, |m| m.refs_hidden = hidden).is_ok()
            && let Some(e) = self.entries.iter_mut().find(|e| e.id == id)
        {
            e.meta.refs_hidden = hidden;
        }
        self.sync_links(cx);
        cx.notify();
    }

    /// 主页's search box, centred on the pane but never over what sits on
    /// either side of it: the title (and, with the sidebar hidden, the
    /// traffic lights and 展开侧栏) on the left, + on the right. Below a
    /// usable width it is None and the caller shows a search icon instead.
    pub(crate) fn header_search(&self, window: &Window, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let main = f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. };
        let title = crate::settings::label(crate::settings::text("主页"));
        let title_w = f32::from(window.text_system().shape_line(title.clone().into(), px(17.), &[window.text_style().highlight(HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() }).to_run(title.len())], None).width);
        // page_header's left inset (26 with the sidebar, else the traffic
        // lights plus the 4 + 28 + 12 of 展开侧栏), then 16 clear of the title.
        let left = if self.sidebar_open { 26. } else { theme::TRAFFIC_LIGHTS_W + 44. + if cfg!(target_os = "windows") { 32. } else { 0. } } + title_w + 16.;
        // + is 32 wide with an 8pt inset; 16 clear of it.
        let right = HEADER_ACTIONS_H + crate::ui::header_right_inset() + 16.;
        let half = (main / 2. - left).min(main / 2. - right);
        let width = (main - 320.).min(2. * half).min(360.);
        if width < 120. { return None }
        Some(crate::ui::toolbar_control(div().id("header-search").absolute().left(px((main - width) / 2.)).top(px(8.))
            .w(px(width)).h(px(32.)).no_drag().flex().items_center().gap(px(8.)).px(px(10.))
            .bg(Theme::fill_hover())
            .cursor_pointer(), 9., Theme::fill_hover_strong, None)
            .child(crate::ui::toolbar_icon("search", 15.))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(Theme::text_caption()).child(crate::settings::tr("搜索文档…", "Search documents…")))
            .when(width >= 200., |d| d.child(div().flex_none().text_size(px(11.)).text_color(Theme::text_placeholder()).child(crate::shortcuts::hint("workspace::FocusSearch", "⌘ + J").replace(" + ", " "))))
            .tip(crate::settings::tr("搜索文档", "Search documents"), Some(crate::shortcuts::hint("workspace::FocusSearch", "⌘ + J")))
            .on_click(cx.listener(|w, _, window, cx| w.open_search(window, cx))))
    }

    fn top_bar(&self, _window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .editor
            .as_ref()
            .map(|e| display_title(&e.read(cx).core.doc.title.text))
            .unwrap_or_default();
        let save_error = match &self.status {
            SaveStatus::Failed(error) => Some(error.clone()),
            _ => None,
        };
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let collaboration_paused = crate::cloud_sync::shared::editing_paused(cx);
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let collaboration_paused = false;
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let sharing = self.sharing_toolbar(_window, cx);
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let sharing = None::<Div>;
        // Independent icon controls, with equal top and right insets.
        let seg = crate::ui::header_icon;
        let divider = || div().flex_none().w(px(1.)).h(px(18.)).bg(Theme::line_divider());
        // Breadcrumbs: the parent pages, each linking to itself, with the
        // middle ones folded into "…" when the chain is long.
        let chain: Vec<(String, String, Option<String>)> = self
            .current
            .as_ref()
            .map(|id| self.ancestors(id).iter().map(|e| (e.id.clone(), display_title(&e.title), e.icon.clone())).collect())
            .unwrap_or_default();
        let mut crumbs: Vec<AnyElement> = vec![];
        let sep = || div().mx(px(3.)).flex_none().child(icon("chevron-right", 12., Theme::icon())).into_any_element();
        for (i, (id, name, em)) in chain.iter().enumerate() {
            if chain.len() > 2 && i > 0 && i < chain.len() - 1 {
                if i == 1 {
                    let hidden: Vec<&str> = chain[1..chain.len() - 1].iter().map(|c| c.1.as_str()).collect();
                    crumbs.push(
                        div()
                            .id("crumb-more")
                            .no_drag()
                            .flex_none()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .text_color(Theme::text_caption())
                            .tip(&hidden.join(" / "), None)
                            .child("…")
                            .into_any_element(),
                    );
                    crumbs.push(sep());
                }
                continue;
            }
            let id = id.clone();
            crumbs.push(
                div()
                    .id(SharedString::from(format!("crumb-{id}")))
                    .no_drag()
                    .min_w(px(28.))
                    .max_w(px(200.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(Theme::text_caption())
                    .hover(|s| s.text_color(Theme::primary_500()))
                    .when_some(em.clone(), |d, em| {
                        d.child(div().size(px(20.)).mr(px(4.)).flex_none().flex().items_center().justify_center().text_size(px(16.)).child(em))
                    })
                    .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(name.clone())))
                    .on_click(cx.listener(move |w, _, window, cx| w.open(&id, window, cx)))
                    .into_any_element(),
            );
            crumbs.push(sep());
        }
        let record_more = self.more_btn_bounds.clone();
        div()
            .id("top-bar").relative()
            .when_some(crate::ui::toolbar_color("toolbar.bg"), |d, c| d.bg(c))
            .h(px(HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .pr(px(crate::ui::header_right_inset()))
            // Clear of the traffic lights, which sit over the note when the sidebar is closed.
            .pl(if self.sidebar_open { px(18.) } else { px(theme::TRAFFIC_LIGHTS_W) })
            .title_bar()
            .when(!self.sidebar_open, |d| {
                d.child(
                    crate::ui::toolbar_control(div()
                        .id("open-sidebar")
                        .no_drag()
                        .flex_none()
                        .ml(px(4.))
                        .size(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer(), 8., Theme::fill_hover_strong, None)
                        .tip(crate::settings::tr("展开侧栏", "Show sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                        .child(crate::ui::toolbar_icon("panel-left-open", 17.))
                        .on_click(cx.listener(|w, _, _, cx| {
                            w.sidebar_open = true;
                            cx.notify();
                        })),
                )
                .child(
                    crate::ui::toolbar_control(div().id("header-home").no_drag().flex_none().ml(px(4.)).size(px(28.))
                        .flex().items_center().justify_center().cursor_pointer(),
                        8., Theme::fill_hover_strong, Some(Theme::fill_pressed))
                        .tip(crate::settings::text("返回主页"), None)
                        .child(crate::ui::toolbar_icon("home", 18.))
                        .on_click(cx.listener(|w, _, _, cx| w.show_view(View::Home, cx))),
                )
                .child(div().mx(px(12.)).child(divider()))
            })
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .h(px(20.))
                            .min_w_0()
                            .flex()
                            .items_center()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .children(crumbs)
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .line_height(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Theme::text())
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(ui_text(title)),
                            ),

                    )
                    .when_some(save_error, |d, error| d.child(
                        div().min_w_0().max_w(px(220.)).text_size(px(12.)).line_height(px(18.))
                            .text_color(theme::c(craft::R400)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(error),
                    ))
                    .when(collaboration_paused, |d| d.child(div().flex_none().text_size(px(12.))
                        .text_color(Theme::text_caption()).child(crate::settings::tr("协作已暂停", "Collaboration paused")))),
            )
            .child(div().flex_1().min_w(px(16.)))
            .child(
                div().gap(px(4.))
                    .h(px(HEADER_ACTIONS_H)).flex_none()
                    .flex()
                    .items_center()
                    .children(sharing)
                    .when(!crate::in_shared_note() && self.current.is_some(), |d| {
                        d.child(
                            // No tooltip while its dropdown is open; the menu hangs
                            // from this button's right edge.
                            seg("doc-more", "ellipsis")
                                .relative()
                                .when(self.doc_menu, |d| d.bg(crate::ui::toolbar_color("toolbar.button.hover").unwrap_or_else(Theme::fill_hover)))
                                .when(!self.doc_menu, |d| d.tip(crate::settings::tr("更多", "More"), None))
                                .child(canvas(move |b, _, _| record_more.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                                .on_click(cx.listener(|w, _, _, cx| {
                                    w.doc_menu = !w.doc_menu;
                                    w.doc_sub = None;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        seg("doc-search", "search")
                            .tip_label(crate::shortcuts::keys(crate::settings::text("搜索(⌘ + J)")).into(), TipSide::Below)
                            .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(FocusSearch), cx))),
                    )
                    .when(!crate::in_shared_note(), |d| d.child(
                        // Straight to a new document: no 新建 menu, as 导入 is rare.
                        seg("doc-new", "plus").tip(crate::settings::tr("新建文档", "New document"), None).on_click(cx.listener(|w, _, window, cx| w.new_doc(window, cx))),
                    )).into_any_element()
            )
    }

    fn render_shared_waiting(&self, cx: &App) -> AnyElement {
        let revoked = {
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            { crate::cloud_sync::shared::access(cx) == Some(crate::cloud_sync::shared::Access::Revoked) }
            #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
            { let _ = cx; false }
        };
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if crate::cloud_sync::shared::editing_paused(cx) && !revoked {
            return div().size_full().flex().items_center().justify_center().text_color(Theme::text_caption())
                .child(crate::cloud_trial::access_required()).into_any_element();
        }
        let message = if revoked { crate::settings::tr("共享访问已结束", "Access to this shared note has ended") }
            else { crate::settings::tr("正在接收共享笔记", "Waiting for the shared note") };
        div().size_full().flex().items_center().justify_center()
            .text_color(Theme::text_caption()).child(message).into_any_element()
    }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
pub(crate) fn cloud_open_positions(cx: &mut App) -> Option<HashMap<String, Vec<crate::editor::core::Pos>>> {
    let mut positions: HashMap<_, Vec<_>> = HashMap::new();
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let selection = window.update(cx, |ws, _, cx| {
            let editor = ws.editor.as_ref().map(|ed| ed.read(cx));
            if ws.unsaved || editor.is_some_and(Editor::composing) { return None; }
            Some(ws.current.clone().zip(editor.map(|ed| ed.core.sel)))
        }).ok()??;
        if let Some((id, selection)) = selection {
            let note = positions.entry(id).or_default();
            note.push(selection.anchor);
            if !selection.is_collapsed() { note.push(selection.head); }
        }
    }
    Some(positions)
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
fn cloud_saved_mirrors(id: &str, doc: &crate::model::Document, cx: &mut App) {
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, _, cx| {
            if ws.unsaved && ws.current.as_deref() == Some(id)
                && ws.editor.as_ref().is_some_and(|editor| editor.read(cx).core.doc == *doc)
            {
                ws.unsaved = false;
                ws.dirty_since = None;
                ws.save_task = None;
                ws.status = SaveStatus::Saved;
                cx.notify();
            }
        });
    }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
pub(crate) fn cloud_active_selection(cx: &mut App) -> crate::cloud_sync::presence::SelectionSource {
    use crate::cloud_sync::presence::SelectionSource::*;
    let Some(window) = (if crate::devtools::scripted() {
        cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>())
    } else { cx.active_window().and_then(|window| window.downcast::<Workspace>()) }) else { return Idle };
    window.update(cx, |ws, window, cx| {
        let (Some(id), Some(editor)) = (&ws.current, &ws.editor) else { return Idle };
        let editor = editor.read(cx);
        if ws.view != View::Doc || !editor.cloud_history
            || (!crate::devtools::scripted() && !editor.focus.is_focused(window)) { return Idle; }
        if ws.unsaved || editor.composing() { return Waiting; }
        Editing(id.clone(), editor.core.sel)
    }).unwrap_or(Idle)
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
pub(crate) fn cloud_account_notice(phase: crate::cloud_sync::SyncPhase, cx: &mut App) {
    let message = if phase == crate::cloud_sync::SyncPhase::SignedOut {
        crate::settings::tr("iCloud 已退出。请在文档库设置中继续。",
            "You’re signed out of iCloud. Continue in Documents settings.")
    } else {
        crate::settings::tr("iCloud 账户已切换。请在文档库设置中继续。",
            "The iCloud account changed. Continue in Documents settings.")
    };
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, _, cx| ws.show_toast_kind(ToastKind::Info, message, cx));
    }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
pub(crate) fn cloud_notes_changed(notes: &[crate::cloud_sync::NoteChange], remote: bool, cx: &mut App) {
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, window, cx| {
            ws.entries = ws.store.list();
            ws.sort();
            let access = crate::cloud_sync::shared::access(cx);
            let access_paused = crate::cloud_sync::shared::editing_paused(cx);
            if access.is_some_and(|access| !access.can_write()) && ws.unsaved {
                if let Some(editor) = &ws.editor {
                    let _ = ws.store.recover_later(&editor.read(cx).core.doc);
                }
                ws.unsaved = false;
                ws.dirty_since = None;
                ws.save_task = None;
                ws.status = SaveStatus::Idle;
                if let (Some(editor), Some(id)) = (&ws.editor, &ws.current)
                    && let Ok(doc) = ws.store.load(id)
                {
                    editor.update(cx, |editor, cx| editor.receive_document(&crate::editor::core::Core::new(doc), cx));
                }
            }
            if ws.current.as_ref().is_some_and(|id| !ws.entries.iter().any(|e| &e.id == id)) {
                if let Some(id) = ws.current.take() { ws.store.forget_loaded(&id); }
                ws.sharing.panel_open = false;
                ws.editor = None;
                ws.editor_sub = None;
                ws.save_task = None;
                if ws.view == View::Doc { ws.view = View::Home; }
            }
            if ws.folder_current.as_ref().is_some_and(|id| !ws.entries.iter().any(|e| &e.id == id)) {
                ws.folder_current = None;
                if ws.view == View::Folder { ws.view = View::Home; }
            }
            if ws.view == View::Trash { ws.trash = ws.store.list_trash(); }
            if let Some(room) = crate::cloud_sync::shared::active() {
                if ws.current.is_none() && ws.entries.iter().any(|entry| entry.id == room.note) {
                    ws.open(&room.note, window, cx);
                }
                let reading = ws.reading();
                if let Some(editor) = &ws.editor {
                    editor.update(cx, |editor, cx| {
                        let locked = access_paused || access.is_some_and(|access| !access.can_write());
                        if editor.read_only != (reading || locked) { editor.set_read_only(reading || locked, cx); }
                    });
                }
            }
            if let (Some(current), Some(ed)) = (&ws.current, &ws.editor)
                && let Some(change) = notes.iter().find(|change| change.id == *current)
            {
                let _ = ws.store.load_for_edit(current, false);
                let modified = ws.entries.iter().find(|entry| entry.id == *current).map(|e| e.meta.modified);
                ed.update(cx, |editor, cx| {
                    let mut selection = editor.core.sel;
                    let resolve = |pos| change.positions.iter().find(|(from, _)| *from == pos)
                        .map_or(pos, |(_, to)| *to);
                    selection.anchor = resolve(selection.anchor);
                    selection.head = resolve(selection.head);
                    editor.receive_document_at(&crate::editor::core::Core::new(change.document.clone()), selection, cx);
                    if let Some(modified) = modified { editor.modified = modified; }
                });
                if remote {
                    if !crate::in_shared_note() {
                        ws.show_toast(crate::settings::tr("已合并 iCloud 上的改动", "Merged changes from iCloud"), cx);
                    }
                } else {
                    ws.unsaved = false;
                    ws.save_task = None;
                    ws.dirty_since = None;
                    ws.status = SaveStatus::Saved;
                }
            }
            ws.sync_links(cx);
            cx.notify();
        });
    }
}

/// Header and action group share an 8px top/right inset.
pub const HEADER_H: f32 = 48.;
pub(crate) const HEADER_ACTIONS_H: f32 = 32.;

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if self.view == View::Doc && crate::in_shared_note() && window.is_window_active()
            && let Some(editor) = &self.editor
            && let Ok(displayed) = crate::cloud_sync::displayed_shared_updates(&editor.read(cx).core.doc, cx)
            && !displayed.is_empty()
        {
            window.on_next_frame(move |window, cx| {
                if window.is_window_active()
                    && let Err(error) = crate::cloud_sync::mark_shared_updates_read(&displayed, cx)
                { crate::logging::write(&format!("Shared note read state: {error:#}")); }
            });
        }
        // Clicks no longer take focus from the editor (see the root's mouse
        // down), so leaving the document hands it back to the window's keys.
        if self.view != View::Doc && self.editor.as_ref().is_some_and(|e| e.read(cx).focus.is_focused(window)) { window.focus(&self.focus); }
        crate::editor::image::frame_drawn(window);
        self.native_sidebar.sync(if self.sidebar_open { self.sidebar_w(window) } else { 0. }, window);
        let sidebar = if self.sidebar_open { Some(if matches!(self.view, View::Settings | View::Trash) { self.render_settings_sidebar(window, cx) } else { self.render_sidebar(window, cx) }) } else { None };
        let row_menu = self.render_row_menu(window, cx);
        let resize = self.sidebar_open.then(|| self.render_resize_handle(window, cx));
        let resize_overlay = self.render_resize_overlay(cx);
        let folder_drag_overlay = self.render_folder_drag_overlay(cx);
        let home_drag_overlay = self.render_home_drag_overlay(window, cx);
        let file_drop = self.render_file_drop(window, cx);
        let create_menu = self.render_create_menu(cx);
        let import_menu = self.render_import_menu(cx);
        let search = self.render_search(window, cx);
        let doc_menu = self.render_doc_menu(window, cx);
        let doc_info = self.render_doc_info(cx);
        let confirm = self.render_confirm(window, cx);
        let trash_menu = self.render_trash_menu(cx);
        let home_menus = self.render_home_menus(cx);
        let theme_menu = self.render_theme_menu(cx);
        let toast = self.render_toast(window);
        let mode_menu = self.render_mode_menu(cx);
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let sharing_panel = self.render_sharing_toolbar_panel(window, cx);
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let sharing_panel = None::<AnyElement>;
        let main = match self.view {
            View::Doc => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(self.top_bar(window, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .children(self.editor.clone()),
                        )
                        .children(self.render_shortcuts(cx)),
                )
                .into_any_element(),
            View::Home if crate::in_shared_note() => self.render_shared_waiting(cx),
            View::Home => self.render_home(window, cx),
            View::Folder => self.render_folder(window, cx),
            View::Trash => self.render_trash(cx),
            View::Settings => self.render_settings(window, cx),
        };
        let move_dlg = self.render_move_dialog(cx);
        let export_md = self.render_export_md(cx);
        // Only the note stands out: a white plane flush with the window's top,
        // right and bottom edges. The sidebar sits sunken on the window
        // surface beside it; the change of value is the only line between
        // them (no gutter, radius, border or shadow).
        // A document sits on its theme's page colour.
        let page = if self.view == View::Doc { crate::editor_theme::Theme::bg_body() } else { Theme::bg_body() };
        let note = div().id("note").relative().flex_1().min_w_0().h_full().flex().bg(page).child(main)
            // Files from the Finder dropped on 主页 or a folder's page are imported there.
            .when(!crate::in_shared_note() && matches!(self.view, View::Home | View::Folder), |d| d
                .on_drag_move(cx.listener(|w, ev: &DragMoveEvent<ExternalPaths>, _, cx| w.file_drag_move(ev, cx)))
                .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| w.file_dropped(paths, window, cx))))
            .children(file_drop);
        div()
            .id("workspace")
            .key_context("Workspace")
            .capture_action(cx.listener(|w, _: &crate::editor::keymap::Undo, window, cx| w.capture_file_history(false, window, cx)))
            .capture_action(cx.listener(|w, _: &crate::editor::keymap::Redo, window, cx| w.capture_file_history(true, window, cx)))
            .track_focus(&self.focus)
            // A click on what takes no input (title bar, page margins, the
            // sidebar's blank space) leaves the caret in the document, also
            // the click that brings the window back (#59). Runs before the
            // root takes focus; anything inside that takes focus did so first.
            .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| {
                if w.view == View::Doc && w.editor.as_ref().is_some_and(|e| e.read(cx).focus.is_focused(window)) { window.prevent_default(); }
            }))
            .relative()
            .size_full()
            .flex()
            .bg(Theme::window())
            .font_family(theme::UI_FONT)
            .text_color(Theme::text())
            // Window keys act on the window they were pressed in, not on
            // whichever window macOS considers active.
            .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, window, cx| w.track_sidebar_press(ev, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|w, _, window, cx| w.finish_sidebar_click(window, cx)))
            .on_action(cx.listener(|w, _: &CloseWindow, window, cx| {
                crate::quick_note::persist(cx);
                if w.save_before(cx, |_, window, _| window.remove_window()) { window.remove_window(); }
            }))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &Zoom, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(cx.listener(|w, _: &OpenSettings, _, cx| w.show_view(View::Settings, cx)))
            .on_action(cx.listener(|w, _: &OpenAbout, _, cx| w.open_settings(crate::settings::Page::About, cx)))
            .on_action(cx.listener(|w, _: &CheckForUpdates, _, cx| { w.open_settings(crate::settings::Page::About, cx); crate::update::check(true, cx); }))
            .on_action(cx.listener(|w, _: &GoHome, _, cx| { if !crate::in_shared_note() { w.show_view(View::Home, cx); } }))
            .on_action(cx.listener(|w, _: &NewDoc, window, cx| w.new_doc(window, cx)))
            .on_action(cx.listener(|w, _: &ImportDoc, window, cx| { if !crate::in_shared_note() { w.open_import(window, cx); } }))
            .on_action(cx.listener(|w, _: &OpenFile, window, cx| { if !crate::in_shared_note() { w.open_file_dialog(window, cx); } }))
            .on_action(cx.listener(|w, _: &ImportFolder, window, cx| { if !crate::in_shared_note() { w.open_import_folder(window, cx); } }))
            .on_action(cx.listener(|w, _: &ToggleSidebar, _, cx| {
                if crate::in_shared_note() { return; }
                w.sidebar_open = !w.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|w, _: &FindInDoc, window, cx| w.find_in_doc(false, window, cx)))
            .on_action(cx.listener(|w, _: &ReplaceInDoc, window, cx| {
                let replace = !w.reading();
                w.find_in_doc(replace, window, cx)
            }))
            .on_action(cx.listener(|w, _: &crate::modes::EditMode, window, cx| w.set_mode(false, window, cx)))
            .on_action(cx.listener(|w, _: &crate::modes::ReadMode, window, cx| w.set_mode(true, window, cx)))
            .on_action(cx.listener(|w, _: &crate::modes::ToggleShortcuts, _, cx| {
                w.shortcuts = !w.shortcuts;
                cx.notify();
            }))
            .on_key_down(cx.listener(|w, ev: &KeyDownEvent, window, cx| {
                // The theme dropdown: ↑ ↓ preview, ↩ applies, Esc closes it (not 设置).
                if w.theme_menu.is_some() && w.theme_menu_key(&ev.keystroke.key, cx) {
                    cx.stop_propagation();
                    return;
                }
                // Esc leaves 设置 (unless its filter field has the caret).
                if ev.keystroke.key == "escape" && w.view == View::Settings && !w.settings_filter.read(cx).is_focused(window) {
                    w.close_settings(cx);
                    cx.stop_propagation();
                    return;
                }
                if ev.keystroke.key == "escape" && (w.doc_info.is_some() || w.doc_menu || w.mode_menu) {
                    w.doc_info = None;
                    w.mode_menu = false;
                    w.doc_menu = false;
                    w.doc_sub = None;
                    cx.stop_propagation();
                    cx.notify();
                }
                #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                if ev.keystroke.key == "escape" && w.sharing.panel_open {
                    w.sharing.panel_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            // Esc leaves 批量操作 (closing its menu or confirmation first), even from the editor.
            .capture_key_down(cx.listener(|w, ev: &KeyDownEvent, _, cx| {
                if w.file_history.busy && !(ev.keystroke.modifiers.secondary() && matches!(ev.keystroke.key.as_str(), "z" | "y")) { cx.stop_propagation(); return; }
                if ev.keystroke.key == "escape" && w.has_home_drag() {
                    w.cancel_home_drag(cx); cx.stop_propagation(); return;
                }
                if ev.keystroke.key == "escape" && w.folder_drag.active.is_some() {
                    w.end_folder_drag(true, cx); cx.stop_propagation(); return;
                }
                if w.record_shortcut(ev, cx) { return; }
                // Esc cancels a confirmation dialog.
                if ev.keystroke.key == "escape" && (w.confirm.is_some() || w.export_md.is_some()) {
                    w.confirm = None;
                    w.after_save = None;
                    w.export_md = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if ev.keystroke.key != "escape" || w.batch.is_none() || w.search_open || w.rename.is_some() {
                    return;
                }
                if w.row_menu.is_some() {
                    w.row_menu = None;
                } else {
                    w.batch = None;
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_action(cx.listener(|w, _: &FocusSearch, window, cx| {
                if w.search_open { w.close_search(window, cx); } else { w.open_search(window, cx); }
                cx.stop_propagation();
            }))
            .children(sidebar)
            .child(note)
            .children(resize)
            .children(crate::ui::window_controls(window))
            .children(row_menu)
            .children(doc_menu)
            .children(sharing_panel)
            .children(mode_menu)
            .children(doc_info)
            .children(trash_menu)
            .children(home_menus)
            .children(theme_menu)
            .children(confirm)
            .children(move_dlg)
            .children(export_md)
            .children(search)
            .children(resize_overlay)
            .children(folder_drag_overlay)
            .when(self.file_history.busy, |d| d.child(deferred(div().id("file-history-pending").absolute().inset_0().occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())).with_priority(20)))
            .children(home_drag_overlay)
            .children(create_menu)
            .children(import_menu)
            .children(toast)
    }
}

/// Fault injection against real window/save code, available only in the harness build.
#[cfg(feature = "devtools")]
#[derive(Clone)]
struct SaveSafetyQa(Vec<(WindowHandle<Workspace>, String)>);
#[cfg(feature = "devtools")]
impl Global for SaveSafetyQa {}

#[cfg(feature = "devtools")]
pub(crate) fn check_save_safety(phase: &str, cx: &mut App) {
    use crate::model::{Document, RichText};
    let doc = |title: &str| Document { title: RichText::plain(title), ..Default::default() };
    if phase == "setup" {
        let mut store = cx.global::<SharedStore>().clone();
        let mut windows = vec![];
        for name in ["A", "B", "C"] {
            let id = store.create_from(&doc(name)).unwrap();
            let open_id = id.clone();
            let handle = cx.open_window(WindowOptions { focus: false, ..Default::default() }, |window, cx| {
                cx.new(|cx| Workspace::new_document_window(Some(open_id), window, cx))
            }).unwrap();
            handle.update(cx, |ws, _, cx| {
                ws.editor.as_ref().unwrap().update(cx, |e, cx| e.receive_document(&crate::editor::core::Core::new(doc(&format!("local-{name}"))), cx));
                ws.unsaved = true;
            }).unwrap();
            if name != "C" {
                std::fs::write(crate::store::Store::root_dir().join("docs").join(format!("{id}.xml")), crate::docxml::serialize(&doc(&format!("disk-{name}")))).unwrap();
            }
            windows.push((handle, id));
        }
        quit(cx);
        for (handle, _) in &windows[..2] {
            handle.update(cx, |ws, _, _| assert!(ws.unsaved && matches!(ws.confirm, Some(Confirm::SaveFailed)))).unwrap();
        }
        assert_eq!(store.load(&windows[2].1).unwrap().title.text, "local-C");
        windows[0].0.update(cx, |ws, window, cx| {
            ws.resolve_save_failed(true, window, cx);
            assert!(ws.unsaved);
            ws.confirm = None;
            ws.resolve_save_failed(false, window, cx);
            assert!(!ws.unsaved);
            assert_eq!(ws.editor.as_ref().unwrap().read(cx).core.doc.title.text, "disk-A");
        }).unwrap();
        cx.set_global(SaveSafetyQa(windows));
        eprintln!("save-safety: all windows checked; successful sibling saved; retry and reload checked");
    } else {
        let windows = cx.global::<SaveSafetyQa>().0.clone();
        windows[1].0.update(cx, |ws, window, cx| {
            assert!(ws.unsaved && matches!(ws.confirm, Some(Confirm::SaveFailed)));
            ws.after_save = None;
            ws.save_failed_copy(window, cx);
            assert!(!ws.unsaved);
            assert_eq!(ws.editor.as_ref().unwrap().read(cx).core.doc.title.text, "disk-B");
            assert!(ws.store.list().iter().any(|e| e.title.starts_with("local-B")));
        }).unwrap();
        windows[0].0.update(cx, |ws, _, cx| {
            ws.editor.as_ref().unwrap().update(cx, |e, cx| e.receive_document(&crate::editor::core::Core::new(doc("after-reload")), cx));
            ws.unsaved = true;
            assert!(ws.flush(cx));
            assert_eq!(ws.store.load(ws.current.as_ref().unwrap()).unwrap().title.text, "after-reload");
        }).unwrap();
        eprintln!("save-safety: PASS; remaining conflict blocked quit; copy preserved; edit after reload saved cleanly");
        for (handle, _) in windows { handle.update(cx, |_, window, _| window.remove_window()).unwrap(); }
    }
}
