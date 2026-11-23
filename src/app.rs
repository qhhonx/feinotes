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
use std::time::Duration;
use web_time::Instant;

/// Autosave waits for this pause in editing, but never longer than
/// SAVE_MAX_DELAY after the oldest change not yet on disk.
const SAVE_DELAY: Duration = Duration::from_millis(500);
const SAVE_MAX_DELAY: Duration = Duration::from_secs(2);

actions!(workspace, [NewDoc, OpenFile, OpenFolder, ImportDoc, ImportFolder, ToggleSidebar, FocusSearch, FindInDoc, ReplaceInDoc, OpenSettings, OpenAbout, CheckForUpdates, GoHome, CloseWindow, Quit,
    ReplayOnboarding, HideApp, HideOthers, ShowAll, Minimize, Zoom, ToggleFullScreen]);
// A confirmation dialog's keys, in its own key context: ↩ is the action button, Esc 取消.
actions!(confirm, [ConfirmOk, ConfirmCancel]);

thread_local! {
    /// The menu bar is for a Markdown file: 插入 / 格式 leave out what it can't hold.
    static MENU_MARKDOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The menu bar follows the document in the window in front; rebuilt only
/// when that changes between a Markdown file and anything else.
pub fn menus_follow(markdown: bool, cx: &mut App) {
    if MENU_MARKDOWN.replace(markdown) != markdown { set_menus(cx); }
}

/// The menu bar, in the interface language.
pub fn set_menus(cx: &mut App) { cx.set_menus(menu_bar(MENU_MARKDOWN.get())); }

/// The menus; for a Markdown file without 提醒, 下划线 and 对齐, which it can't keep.
fn menu_bar(markdown: bool) -> Vec<Menu> {
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
    vec![
        Menu { disabled: false, name: name.into(), items: app_items },
        Menu {
            name: tr("文件", "File").into(),
            items: vec![
                MenuItem::action(tr("新建文档", "New Document"), NewDoc),
                MenuItem::action(tr("新建文档窗口", "New Document Window"), crate::quick_note::QuickNew),
                MenuItem::action(tr("打开 Markdown 文件…", "Open Markdown File…"), OpenFile),
            ].into_iter().chain((!crate::FEI_NOTES).then(|| MenuItem::action(tr("打开文件夹…", "Open Folder…"), OpenFolder))).chain([
                MenuItem::action(tr("导入为文档…", "Import as Document…"), ImportDoc),
                MenuItem::action(tr("导入文件夹…", "Import Folder…"), ImportFolder),
                MenuItem::separator(),
                MenuItem::action(tr("关闭窗口", "Close Window"), CloseWindow),
            ]).collect(),
            disabled: false,
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
                    MenuItem::action(tr("拷贝为 Markdown", "Copy as Markdown"), k::CopyMarkdown),
                    MenuItem::os_action(tr("粘贴", "Paste"), k::Paste, OsAction::Paste),
                    MenuItem::os_action(tr("全选", "Select All"), k::SelectAll, OsAction::SelectAll),
                    MenuItem::separator(),
                    MenuItem::action(tr("整理本文空格", "Add Spaces Between Chinese and English"), k::SpaceCjk),
                ]
            },
            disabled: false,
        },
        // 插入 and 格式: the menu-bar home of everything the page's
        // right-click menu offers, so no action is reachable only by shortcut.
        Menu {
            name: tr("插入", "Insert").into(),
            items: {
                use crate::editor::keymap as k;
                let mut items = vec![
                    MenuItem::action(tr("链接", "Link"), k::Link),
                    MenuItem::action(tr("表格", "Table"), k::InsertTable),
                    MenuItem::action(tr("分隔线", "Divider"), k::InsertDivider),
                ];
                if !markdown { items.push(MenuItem::action(tr("提醒", "Reminder"), k::InsertReminder)); }
                items
            },
            disabled: false,
        },
        Menu {
            name: tr("格式", "Format").into(),
            items: {
                use crate::editor::keymap as k;
                let mut items = vec![MenuItem::action(tr("粗体", "Bold"), k::Bold), MenuItem::action(tr("斜体", "Italic"), k::Italic)];
                if !markdown { items.push(MenuItem::action(tr("下划线", "Underline"), k::Underline)); }
                items.extend([
                    MenuItem::action(tr("删除线", "Strikethrough"), k::Strike),
                    MenuItem::action(tr("行内代码", "Inline code"), k::InlineCode),
                    MenuItem::separator(),
                    MenuItem::submenu(Menu {
                        name: tr("转换为", "Turn into").into(),
                        items: vec![
                            MenuItem::action(tr("正文", "Text"), k::TurnText),
                            MenuItem::action(tr("一级标题", "Heading 1"), k::TurnH1),
                            MenuItem::action(tr("二级标题", "Heading 2"), k::TurnH2),
                            MenuItem::action(tr("三级标题", "Heading 3"), k::TurnH3),
                            MenuItem::action(tr("无序列表", "Bulleted list"), k::TurnBullet),
                            MenuItem::action(tr("有序列表", "Numbered list"), k::TurnOrdered),
                            MenuItem::action(tr("任务", "Task"), k::TurnTodo),
                            MenuItem::action(tr("引用", "Quote"), k::TurnQuote),
                            MenuItem::action(tr("代码块", "Code block"), k::TurnCode),
                        ],
                        disabled: false,
                    }),
                ]);
                if !markdown {
                    items.push(MenuItem::submenu(Menu {
                        name: tr("对齐", "Align").into(),
                        items: vec![
                            MenuItem::action(tr("左对齐", "Align left"), k::AlignLeft),
                            MenuItem::action(tr("居中对齐", "Align center"), k::AlignCenter),
                            MenuItem::action(tr("右对齐", "Align right"), k::AlignRight),
                        ],
                        disabled: false,
                    }));
                }
                items
            },
            disabled: false,
        },
        Menu {
            name: tr("窗口", "Window").into(),
            items: vec![
                MenuItem::action(tr("最小化", "Minimize"), Minimize),
                MenuItem::action(tr("缩放", "Zoom"), Zoom),
                MenuItem::separator(),
                MenuItem::action(tr("进入 / 退出全屏幕", "Enter / Exit Full Screen"), ToggleFullScreen),
            ],
            disabled: false,
        },
    ]
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-z", crate::editor::keymap::Undo, Some("Workspace")),
        #[cfg(target_os = "ios")]
        KeyBinding::new("cmd-z", crate::editor::keymap::Undo, Some("Workspace")),
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-shift-z", crate::editor::keymap::Redo, Some("Workspace")),
        #[cfg(target_os = "ios")]
        KeyBinding::new("cmd-shift-z", crate::editor::keymap::Redo, Some("Workspace")),
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-y", crate::editor::keymap::Redo, Some("Workspace")),
        #[cfg(target_os = "ios")]
        KeyBinding::new("cmd-y", crate::editor::keymap::Redo, Some("Workspace")),
        KeyBinding::new("cmd-n", NewDoc, None),
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-shift-n", crate::quick_note::QuickNew, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        // For trying the first-launch pages; not listed in Settings › Shortcuts.
        KeyBinding::new("cmd-alt-shift-o", ReplayOnboarding, None),
        KeyBinding::new("cmd-shift-i", ImportDoc, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", OpenFolder, None),
        KeyBinding::new("cmd-alt-home", GoHome, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-]", ToggleSidebar, None),
        #[cfg(not(target_os = "ios"))]
        KeyBinding::new("cmd-\\", ToggleSidebar, None),
        KeyBinding::new("cmd-j", FocusSearch, None),
        KeyBinding::new("cmd-f", FindInDoc, None),
        KeyBinding::new("cmd-shift-h", ReplaceInDoc, None),
        KeyBinding::new("cmd-q", Quit, None),
        // The dialog takes focus while it is up, so ↩ never reaches the document.
        KeyBinding::new("enter", ConfirmOk, Some("Confirm")),
        KeyBinding::new("escape", ConfirmCancel, Some("Confirm")),
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
    #[cfg(not(any(target_os = "macos",target_os = "ios")))]
    cx.bind_keys([KeyBinding::new("f11", ToggleFullScreen, None)]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("cmd-h", ReplaceInDoc, None)]);
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(handle) = cx.active_window().and_then(|w| w.downcast::<Workspace>()) {
            // Key dispatch already borrows this window; close after it returns.
            cx.defer(move |cx| {
                if let Err(e) = handle.update(cx, |ws, window, cx| {
                    crate::quick_note::persist(cx);
                    ws.remember_scroll(cx);
                    if ws.save_before(cx, |_, window, _| window.remove_window()) { window.remove_window(); }
                }) { eprintln!("无法关闭文档窗口：{e}"); }
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

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
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
    pub corner: Anchor,
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
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    Shared,
}

/// A confirmation dialog and the pages it is about.
#[derive(Debug)]
pub enum Confirm {
    /// 删除 (to the 回收站); `batch` for the 批量操作 selection.
    Delete { ids: Vec<String>, batch: bool },
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    DeleteSharedFolder { room: crate::cloud_sync::shared::Room, id: String, title: String },
    /// 回收站 → 恢复.
    Restore(String),
    /// 回收站 → 彻底删除.
    Purge(String),
    /// 回收站 → 清空回收站: every trashed page, permanently.
    EmptyTrash,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    DiscardOfflineRecovery(String),
    /// 设置 › 文档库 → 清理未使用资源: the sweep's file names and sizes (#237).
    RemoveUnused(Vec<(String, u64)>),
    /// 隐藏引用列表 (with 不再提示) / 显示引用列表.
    Refs { show: bool, no_ask: bool },
    /// The open document could not be written before it was to be replaced
    /// (another document, 新建, 退出): 重试 / 放弃修改, then carry on.
    SaveFailed,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    StartCloudTrial,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    SwitchCloudLibrary { cloud: bool, copy_local: bool },
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    RestoreLibraryBackup(Box<crate::cloud_sync::backup::Prepared>),
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    CloudVersions { id: String, versions: Vec<(i64, String, String)>, selected: Option<i64> },
    /// The library should move to real folders, and the user is asked first
    /// (it is synced, or used from another computer): 改用文件夹 / 稍后.
    Migrate(crate::store::ConsentReason),
    /// 从侧栏移除 a root folder (key, name): the folder and its files stay.
    RemoveRoot(String, String),
    /// Dropped on the sidebar from outside: import it into the library there?
    Import { what: DropImport, target: crate::sidebar_drop::SidebarDropAt },
}

/// What a sidebar drop would import: files and folders from the Finder, or a 本机文件.
#[derive(Clone, Debug)]
pub enum DropImport { Files(Vec<std::path::PathBuf>), Local(String) }

/// What to do once the open document is safely on disk (or its changes given up).
pub type AfterSave = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

#[derive(Default)]
/// One root's lists as a background writer left them (`SharedStore::merge_entries`).
struct ListedDocs { root: String, entries: Vec<crate::store::Entry>, trash: Vec<crate::store::TrashEntry>, warnings: Vec<String> }
struct DeletedDocs { deleted: Vec<String>, errors: Vec<String>, changes: Vec<crate::store::FileDelta> }

/// Inline 重命名 of a sidebar row.
pub struct Rename {
    pub id: String,
    pub(crate) sidebar_section: Option<crate::sidebar::SidebarSection>,
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
    #[cfg(target_os = "ios")]
    pub(crate) mobile_settings_root: bool,
    #[cfg(not(target_os = "ios"))]
    pub(crate) settings_return: View,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_settings_parent: Option<crate::settings::Page>,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) cloud_settings_from_share: bool,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_settings_return: View,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_navigation: crate::mobile::Navigation,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_batch: Option<crate::mobile_batch::BatchSession>,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_reorder: Option<crate::mobile_reorder::ReorderSession>,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_information: Option<crate::docinfo::InformationSession>,
    /// 版本历史: snapshots kept per document (1–30), cached for the settings page.
    pub history_limit: u32,
    /// 外观's theme dropdown while open: the theme its preview shows.
    pub theme_menu: Option<String>,
    /// Where the theme dropdown's button was drawn last frame.
    pub theme_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// 通用's language dropdown while open: the highlighted language.
    pub lang_menu: Option<crate::settings::Lang>,
    pub lang_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// 通用's other dropdowns (快捷新建, 网页搜索, 打开链接): the open one, and
    /// where each button was drawn last frame.
    pub pref_menu: Option<crate::settings::PrefMenu>,
    pub pref_anchors: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<&'static str, Bounds<Pixels>>>>,
    /// 外观's font list while open (its search field lives in here), and
    /// where each of its two buttons was drawn last frame.
    pub font_menu: Option<crate::settings::FontMenu>,
    pub font_anchors: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<&'static str, Bounds<Pixels>>>>,
    /// 快捷键's filter field.
    pub settings_filter: Entity<TextField>,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub sharing: crate::sharing::Sharing,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub library_switching: bool,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub subscription: crate::settings::Subscription,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub sync_issues: Option<Vec<crate::cloud_sync::SyncIssue>>,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) conflict_review: Option<crate::cloud_sync::conflict_ui::ConflictReview>,
    pub(crate) native_sidebar: crate::native_header::NativeSidebar,
    pub store: SharedStore,
    pub entries: Vec<Entry>,
    pub current: Option<String>,
    // Only explicit New starts a private editor. The first edit promotes its seed through the normal store.
    pub(crate) draft: Option<crate::store::NewNoteDraft>,
    departing_focus: Option<FocusHandle>,
    pub editor: Option<Entity<Editor>>,
    pub search: Entity<TextField>,
    /// Keyboard highlight inside search results.
    pub search_sel: Option<usize>,
    pub sidebar_open: bool,
    /// Dragged width (300 by default, 268 to min(800, window − 540)).
    pub sidebar_width: f32,
    /// The resize handle is being dragged.
    pub sidebar_drag: bool,
    /// The sidebar was open at the last render: opening it in a narrow window
    /// widens the window, narrowing the window below it hides it again.
    sidebar_was_open: bool,
    /// The window width before that widening, until the window takes the new size.
    widen_from: Option<f32>,
    /// Narrowing the window hid the sidebar (not the user): widening it until
    /// the sidebar fits shows it again.
    sidebar_auto_hidden: bool,
    pub folder_current: Option<String>,
    pub create_menu: Option<(crate::folders::CreateTarget, Point<Pixels>)>,
    /// The header's 导入 menu: the folder it imports into, and where it hangs.
    pub import_menu: Option<(Option<String>, Point<Pixels>)>,
    /// The header's … menu on Home (None) or a folder page: where it hangs.
    pub place_menu: Option<(Option<String>, Point<Pixels>)>,
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
    /// Rendered width of the open 「…」 menu, for placing its submenus (#225).
    pub doc_menu_w: Rc<Cell<Option<f32>>>,
    /// The same for the sidebar row menu, whose 下载 submenu sits beside it
    /// (#225).
    pub row_menu_w: Rc<Cell<Option<f32>>>,
    pub doc_info: Option<DocInfo>,
    /// 版本历史 drawer, open for `HistoryView::id`.
    pub history: Option<crate::history::HistoryView>,
    /// A snapshot is on screen: the editor state to put back, and which one.
    pub preview: Option<crate::history::Preview>,
    /// 命名版本: the row's name being typed.
    pub version_rename: Option<crate::history::VersionRename>,
    pub list_scroll: ScrollHandle,
    pub search_scroll: ScrollHandle,
    /// Lowercased titles and bodies and the last query's hits, see [`crate::search::SearchCache`].
    pub search_cache: std::cell::RefCell<crate::search::SearchCache>,
    /// Last-frame bounds of the row menu's 下载 submenu.
    pub row_sub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// 批量操作 mode: the checked docs of 我的文档, in check order.
    pub batch: Option<Vec<String>>,
    /// The open confirmation dialog.
    pub confirm: Option<Confirm>,
    /// The dialog's focus while it is up, and where focus was before it
    /// opened (given back when it closes); `None` while no dialog has it.
    pub confirm_focus: FocusHandle,
    pub confirm_return: Option<Option<WeakFocusHandle>>,
    /// The first-launch pages laid over the window again (⌘⌥⇧O, Settings › About).
    pub onboarding: Option<Entity<crate::onboarding::Onboarding>>,
    pub view: View,
    pub home: crate::home::Home,
    pub settings_scroll: ScrollHandle,
    /// 回收站 rows, refreshed when the page opens or changes.
    pub trash: Vec<crate::store::TrashEntry>,
    /// A 回收站 row's menu: the page and where it opens.
    pub trash_menu: Option<(String, Point<Pixels>, Anchor)>,
    /// The toast and when it appeared.
    pub toast: Option<(ToastKind, SharedString, Instant)>,
    toast_task: Option<Task<()>>,
    /// Folded sidebar sections: 置顶, 我的文档.
    pub folded: [bool; 3],
    /// Root folders folded shut in the sidebar, by key.
    pub folded_roots: std::collections::HashSet<String>,
    /// Expanded pages of the 我的文档 tree.
    pub expanded: HashSet<String>,
    /// Where a drag from outside the tree (the Finder, 本机文件) would land in the sidebar.
    pub sidebar_drop: Option<crate::sidebar_drop::SidebarDropAt>,
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
    /// Navigation emphasis follows pointer/key intent without moving the editor caret.
    pub(crate) sidebar_focused: bool,
    pub(crate) sidebar_window_active: bool,
    pub(crate) sidebar_entry: Option<crate::sidebar::SidebarEntry>,
    pub(crate) sidebar_selection_row: Option<crate::sidebar::SidebarEntry>,
    pub(crate) save_task: Option<Task<()>>,
    /// Writes 最近打开 and 阅读位置 once switching documents stops (#324).
    view_state_task: Option<Task<()>>,
    /// When the oldest edit not yet on disk was made: a save waits for a
    /// pause in typing, but never longer than `SAVE_MAX_DELAY` after it.
    dirty_since: Option<Instant>,
    /// The open document has changes not yet on disk (cleared only by a
    /// successful save or by 放弃修改).
    pub(crate) unsaved: bool,
    /// Held back by a failed save: runs after 重试 succeeds or 放弃修改.
    pub(crate) after_save: Option<AfterSave>,
    pub(crate) editor_sub: Option<Subscription>,
    #[cfg(target_os = "ios")]
    mobile_editor_context: Option<Subscription>,
    _subs: Vec<Subscription>,
}


/// A macOS notification (only logged when scripted).
#[cfg(not(target_os = "ios"))]
fn notify_system(title: &str, body: &str) {
    if crate::devtools::scripted() || cfg!(target_family = "wasm") {
        return eprintln!("notification: {title} / {body}");
    }
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", q(body), q(title));
    std::thread::spawn(move || std::process::Command::new("osascript").arg("-e").arg(script).status());
}

impl Workspace {
    /// Other active windows take the lists a background write left; `trash`
    /// only when that write touched it.
    pub(crate) fn sync_peers(&self, cx: &mut Context<Self>, entries: &[Entry], trash: Option<&[crate::store::TrashEntry]>) {
        self.store.adopt_listing(entries);
        let me = cx.entity_id();
        for handle in cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>()) {
            if !handle.is_active(cx).is_some_and(|yes| yes) || handle.entity(cx).is_ok_and(|e| e.entity_id() == me) { continue }
            let (entries, trash) = (entries.to_vec(), trash.map(<[_]>::to_vec));
            let root = self.store.root().to_path_buf();
            let _ = handle.update(cx, |ws, _, cx| {
                if ws.store.root() != root { return; }
                ws.entries = entries; if let Some(trash) = trash { ws.trash = trash; } ws.sort();
                cx.notify();
            });
        }
    }

    pub(crate) fn is_shared(&self) -> bool { self.store.is_shared() }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_window(false, window, cx)
    }

    pub fn new_window(quick: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window(quick, None, None, window, cx)
    }

    pub fn new_document_window(document: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window(true, document, None, window, cx)
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn new_shared_window(store: SharedStore, note: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window(true, Some(note), Some(store), window, cx)
    }

    pub(crate) fn build_window(quick: bool, document: Option<String>, store: Option<SharedStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_window_with_recent(quick, document, store, true, window, cx)
    }

    pub(crate) fn build_window_with_recent(quick: bool, document: Option<String>, store: Option<SharedStore>,
        open_recent: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        #[cfg(target_os = "ios")]
        cx.observe_self(|workspace,cx| crate::ipad::publish_navigation(workspace,cx)).detach();
        crate::settings::load();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        cx.observe_global_in::<crate::cloud_subscription::PurchaseChanged>(window, |ws, window, cx| {
            ws.subscription_updated(window, cx);
        }).detach();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::SyncStatuses>(|ws, cx| {
            if ws.conflict_review.is_some() && (!crate::cloud_sync::can_review_conflict(ws.store.root(), cx)
                || !ws.current.as_deref().is_some_and(|id| crate::cloud_sync::document_conflict(ws.store.root(), id, cx)))
            { ws.conflict_review = None; }
            if matches!(ws.view, View::Doc | View::Settings | View::Shared | View::Trash) || ws.is_shared() || ws.sharing.sheet.is_some() {
                if ws.sync_issues.is_some()
                    && let Some(root) = ws.settings_sync_root(cx)
                    && let Some(status) = crate::cloud_sync::status(&root, cx) {
                    if status.held == 0 && status.attachments == 0 && status.conflicts == 0 { ws.sync_issues = Some(vec![]); }
                    else if let Ok(issues) = crate::cloud_sync::issues(&root, cx) { ws.sync_issues = Some(issues); }
                }
                cx.notify();
            }
        }).detach();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::activity::ActivityView>(|ws, cx| {
            if ws.shared_batch() {
                let ids = cx.global::<crate::cloud_sync::activity::ActivityView>().0.iter().map(|row| row.room.scope.note.clone()).collect::<std::collections::HashSet<_>>();
                if let Some(selected) = &mut ws.batch { selected.retain(|id| ids.contains(id)); }
            }
            cx.notify();
        }).detach();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        cx.observe_global::<crate::cloud_sync::presence::Views>(|ws, cx| {
            let remote = ws.current.as_ref().and_then(|id| crate::cloud_sync::presence::view(ws.store.root(), cx).and_then(|view| view.notes.get(id)))
                .cloned().unwrap_or_default();
            if let Some(editor) = ws.editor.clone() {
                editor.update(cx, |editor, cx| {
                    if editor.remote_selections != remote {
                        editor.remote_selections = remote;
                        cx.notify();
                    }
                });
            }
            if ws.view == View::Settings || ws.is_shared() { cx.notify(); }
        }).detach();
        crate::settings::check_fonts(cx);
        // 文档字体's download list shows progress and what arrived.
        cx.observe_global::<crate::font_downloads::Tick>(|ws, cx| if ws.font_menu.is_some() { cx.notify() }).detach();
        cx.observe_global::<crate::font_downloads::Changed>(|_, cx| cx.notify()).detach();
        if !cx.has_global::<SharedStore>() { cx.set_global(SharedStore::open().expect("open document library")); }
        let mut store = store.unwrap_or_else(|| cx.global::<SharedStore>().clone());
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
                this.sidebar_window_active = window.is_window_active();
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                crate::cloud_sync::presence::wake(this.store.root(), cx);
                cx.notify();
                if window.is_window_active() {
                    if !this.is_shared() && let Some(id) = &this.current { crate::quick_note::used(id, cx); }
                    if !this.store.file_task_busy() { this.entries = this.store.list(); this.sort(); this.refresh_markdown(Some(window.window_handle().window_id()), cx); }
                    crate::update::check_if_stale(cx); cx.notify();
                }
                else if !this.store.file_task_busy() { this.remember_scroll(cx); if this.drop_pristine_draft(cx) { this.view = View::Home; } this.cancel_home_drag(cx); this.end_folder_drag(true, window, cx); this.cancel_shortcut_recording(cx); this.flush(cx); }
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
                if this.sidebar_window_active { this.remember_scroll(cx); }
                this.flush_view_state();
                if !this.store.file_task_busy() && !this.flush(cx) && let Some(ed) = &this.editor
                    && let Err(error) = this.store.recover_later(&ed.read(cx).core.doc) {
                    crate::logging::write(&format!("Could not preserve unsaved document: {error:#}"));
                }
                async {}
            }),
        ];
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            crate::quick_note::persist(cx);
            this.update(cx, |w, cx| { w.remember_scroll(cx); w.cancel_shortcut_recording(cx); w.flush_view_state(); w.save_before(cx, |_, window, _| window.remove_window()) }).unwrap_or(true)
        });
        if !quick { crate::settings::watch_themes(cx); }
        // A file written into the library from outside (a model writing a
        // note, say) joins it without going away and back: the library's
        // shape is compared every couple of seconds and the list re-read only
        // when it changed. Every root folder is watched the same way.
        cx.spawn(async move |this, cx| {
            let bg = cx.background_executor().clone();
            let shape = |dirs: Vec<std::path::PathBuf>| bg.spawn(async move { dirs.iter().map(|d| crate::store::Store::shape_at(d)).collect::<Vec<u64>>() });
            let dirs = |cx: &mut AsyncApp| this.update(cx, |w, _| w.store.root_dirs()).unwrap_or_default();
            let mut last = shape(dirs(cx)).await;
            let mut last_file = None;
            loop {
                bg.timer(Duration::from_secs(2)).await;
                let now = shape(dirs(cx)).await;
                let Ok(path) = this.update(cx, |w, _| w.current.as_ref()
                    .filter(|id| !w.store.file_task_busy() && w.store.is_markdown(id))
                    .and_then(|id| w.store.file_path(id))) else { break };
                let file = bg.spawn(async move {
                    let path = path?;
                    let meta = fdl_core::vfs::metadata(&path).ok()?;
                    Some((path, meta.modified().ok()?, meta.len()))
                }).await;
                if file != last_file {
                    let _ = this.update(cx, |w, cx| w.refresh_markdown(None, cx));
                    last_file = file;
                }
                if now == last { continue }
                let reloaded = this.update(cx, |w, cx| {
                    // Not mid-rename: the row under the field would be rebuilt.
                    if w.rename.is_some() || w.move_dlg.is_some() || w.store.file_task_busy() { return false }
                    w.entries = w.store.list();
                    w.sort();
                    cx.notify();
                    true
                });
                match reloaded { Ok(true) => last = now, Ok(false) => {}, Err(_) => break }
            }
        })
        .detach();
        // 日期提醒: post what fell due since the last look (not what was
        // missed while the app was closed).
        cx.spawn(async |this, cx| {
            #[cfg(not(target_os = "ios"))]
            let mut last = crate::time::now_ms() as i64;
            loop {
                cx.background_executor().timer(Duration::from_secs(15)).await;
                #[cfg(not(target_os = "ios"))]
                let now = crate::time::now_ms() as i64;
                if this.update(cx, |w, cx| {
                    #[cfg(target_os = "ios")]
                    crate::ipad_notifications::publish(w,cx);
                    #[cfg(not(target_os = "ios"))]
                    w.check_reminders(last,now,cx);
                }).is_err() { break; }
                #[cfg(not(target_os = "ios"))]
                { last = now; }
            }
        })
        .detach();
        let settings = store.settings();
        let sidebar_width = settings.sidebar_width.unwrap_or(theme::SIDEBAR_WIDTH);
        let history_limit = store.history_limit();
        let mut ws = Workspace {
            quick_note: quick,
            shortcut_recording: None,
            shortcut_strokes: vec![],
            shortcut_error: None,
            settings_page: Default::default(),
            #[cfg(target_os = "ios")]
            mobile_settings_root: true,
            #[cfg(not(target_os = "ios"))]
            settings_return: View::Home,
            #[cfg(target_os = "ios")]
            mobile_settings_parent: None,
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            cloud_settings_from_share: false,
            #[cfg(target_os = "ios")]
            mobile_settings_return: View::Home,
            #[cfg(target_os = "ios")]
            mobile_navigation: Default::default(),
            #[cfg(target_os = "ios")]
            mobile_batch: None,
            #[cfg(target_os = "ios")]
            mobile_reorder: None,
            #[cfg(target_os = "ios")]
            mobile_information: None,
            history_limit,
            theme_menu: None,
            theme_anchor: Default::default(),
            lang_menu: None,
            lang_anchor: Default::default(),
            pref_menu: None,
            pref_anchors: Default::default(),
            font_menu: None,
            font_anchors: Default::default(),
            settings_filter,
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            sharing: crate::sharing::Sharing::new(cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            library_switching: false,
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            subscription: Default::default(),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            sync_issues: None,
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            conflict_review: None,
            store,
            entries,
            current: None,
            draft: None,
            departing_focus: None,
            editor: None,
            search,
            search_sel: None,
            sidebar_open: !quick,
            sidebar_width,
            sidebar_was_open: false,
            widen_from: None,
            sidebar_auto_hidden: false,
            sidebar_drag: false,
            folder_current: None,
            create_menu: None,
            import_menu: None,
            place_menu: None,
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
            doc_menu_w: Rc::new(Cell::new(None)),
            row_menu_w: Rc::new(Cell::new(None)),
            doc_info: None,
            history: None,
            preview: None,
            version_rename: None,
            list_scroll: ScrollHandle::new(),
            search_scroll: ScrollHandle::new(),
            search_cache: Default::default(),
            row_sub_bounds: Rc::new(Cell::new(None)),
            batch: None,
            confirm: None,
            confirm_focus: cx.focus_handle(),
            confirm_return: None,
            onboarding: None,
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
            folded_roots: Default::default(),
            expanded: HashSet::new(),
            sidebar_drop: None,
            move_dlg: None,
            export_md: None,
            focus: cx.focus_handle(),
            sidebar_focused: false,
            sidebar_window_active: window.is_window_active(),
            sidebar_entry: None,
            sidebar_selection_row: None,
            save_task: None,
            view_state_task: None,
            dirty_since: None,
            unsaved: false,
            after_save: None,
            editor_sub: None,
            #[cfg(target_os = "ios")]
            mobile_editor_context: None,
            _subs: subs,
        };
        ws.sort();
        ws.load_home();
        // An empty library opens on 主页's empty state; only a quick note
        // window starts a document of its own. The last-read preference
        // changes the document choice, preserving product open/access guards.
        let document = document.or_else(|| (!quick && open_recent && !cfg!(target_family = "wasm") && !crate::settings::prefs().start_home).then(|| {
            ws.entries.iter().filter(|e| e.meta.opened > 0).max_by_key(|e| e.meta.opened)
                .or_else(|| ws.entries.first()).map(|e| e.id.clone())
        }).flatten());
        match document {
            Some(id) => { if !ws.try_open(&id, window, cx) { ws.view = View::Home; } }
            None if quick => ws.new_doc(window, cx),
            None => { ws.view = View::Home; crate::gpui_compat::focus(window,&ws.focus,cx); }
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
            }
        }
        #[cfg(target_os = "ios")]
        crate::ipad_notifications::publish(&ws,cx);
        ws
    }

    /// Most recently modified first.
    pub(crate) fn sort(&mut self) {
        self.reapply_pending_tree_moves();
        self.entries.sort_by(|a, b| b.meta.modified.cmp(&a.meta.modified).then_with(|| a.id.cmp(&b.id)));
        self.sync_note_rows();
    }

    pub fn query(&self, cx: &App) -> String {
        self.search.read(cx).text.trim().to_lowercase()
    }

    pub fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if self.is_shared() && self.sidebar_entry.is_some() {
            let entry = crate::sidebar::SidebarEntry::new(self.shared_sidebar_section(), id);
            if self.try_open(id, window, cx) {
                self.reveal_shared_sidebar_entry(&entry, cx);
                self.sidebar_entry = Some(entry);
            }
            return;
        }
        self.try_open(id, window, cx);
    }

    /// Show `id`; false when it could not be opened (or the open document
    /// could not be saved first, see [`Self::save_before`]).
    pub fn try_open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        self.cancel_sidebar_open();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.sharing.pending_open = None; self.sharing.account_open = None; self.conflict_review = None; }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() && let Some(room) = crate::cloud_sync::shared::original::owner_room(&self.entries, id) {
            let target = id.to_owned();
            self.sharing.pending_open = Some(target.clone());
            cx.defer_in(window, move |ws, window, cx| {
                // A newer navigation may have replaced this workspace before the deferred open.
                if ws.sharing.pending_open.as_ref() == Some(&target)
                    && window.root::<Self>().flatten().is_some_and(|root| root.entity_id() == cx.entity_id()) {
                    ws.sharing.pending_open = None;
                    ws.open_shared_item_interactive(room, target, None, window, cx);
                }
            });
            return true;
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if let Some(room) = crate::cloud_sync::shared::scope(&self.store, cx)
            && ((!room.folder && id != room.note) || !self.entries.iter().any(|entry| entry.id == id)
                || crate::cloud_sync::shared::access(&self.store, cx) == Some(crate::cloud_sync::shared::Access::Revoked))
        { return false; }
        // A snapshot is on screen: back to the live document before anything
        // else touches the editor.
        if self.history.is_some() { self.close_history(cx); }
        // 阅读位置: remember where the document being left was, before the
        // editor is swapped out.
        self.remember_scroll(cx);
        // A folder with a 正文 opens as that document (#357).
        if let Some(note) = self.entries.iter().find(|e| e.id == id).and_then(|e| e.note.clone()) { return self.try_open(&note, window, cx); }
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) { return self.open_folder(id, window, cx); }
        if self.current.as_deref() == Some(id) {
            self.sidebar_entry = None;
            if self.view != View::Doc { self.view = View::Doc; cx.notify(); }
            cx.notify();
            return true;
        }
        let target = id.to_string();
        if !self.save_before(cx, move |w, window, cx| {
            w.open(&target, window, cx);
        }) {
            return false;
        }
        // 版本历史 of a 数据表 (written as it is edited): the file as it is left and as it is
        // opened is kept as a version; a file unchanged since the newest version adds nothing
        let table = |w: &Self, i: &str| w.entries.iter().any(|e| e.id == i && e.meta.kind == crate::store::DocKind::Table);
        if let Some(cur) = self.current.clone() && table(self, &cur) { let _ = self.store.snapshot_table(&cur, true); }
        if table(self, id) { let _ = self.store.snapshot_table(id, true); }
        let shared = !self.document_peers(id, window, cx).is_empty();
        let doc = match self.store.load_for_edit(id, shared) {
            Ok(d) => d,
            Err(e) => {
                crate::logging::write(&format!("Open document: {e:#}"));
                let message = if crate::FEI_NOTES {
                    crate::settings::tr("暂时无法打开文档。请重试。", "Could not open the document. Try again.").into()
                } else {
                    crate::localized_format!("无法打开文档：{}", "Could not open document: {}", crate::settings::diagnostic(&e.to_string()))
                };
                self.show_toast_kind(ToastKind::Error, &message, cx);
                return false;
            }
        };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.close_sharing_for_navigation();
        self.view = View::Doc;
        let meta = self.entries.iter().find(|e| e.id == id).map(|e| e.meta.clone());
        let modified = meta.as_ref().map_or_else(crate::time::now, |m| m.modified);
        let width = meta.as_ref().map(|m| m.width).unwrap_or_default();
        let pinned = meta.as_ref().is_some_and(|m| !m.catalogue_closed);
        let editor = cx.new(|cx| {
            let mut e = Editor::new(doc, modified, width, window, cx);
            e.set_library_root(self.store.document_root(id));
            e.cat_pinned = pinned;
            e.markdown = meta.as_ref().is_some_and(|m| m.markdown);
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            {
                e.cloud_history = !crate::cloud_restore::local_only() && meta.as_ref().is_some_and(|m| !m.local());
                e.remote_selections = crate::cloud_sync::presence::view(self.store.root(), cx)
                    .and_then(|view| view.notes.get(id)).cloned().unwrap_or_default();
            }
            e.board_file = meta.as_ref().is_some_and(|m| m.kind == crate::store::DocKind::Board);
            e.table_file = meta.as_ref().is_some_and(|m| m.kind == crate::store::DocKind::Table);
            if let Some(m) = &meta {
                e.created = m.created;
                e.info = m.info;
                if !e.table_file && (crate::FEI_NOTES || !e.board_file) { e.set_read_only(self.reading_document(id), cx); }
                if e.table_file && !crate::DATATABLE { e.set_read_only(true, cx); }
            }
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            if self.is_shared() {
                e.set_shared_access(self.reading_document(id), crate::cloud_sync::shared::editing_paused(&self.store, cx),
                    crate::cloud_sync::shared::access(&self.store, cx).is_some_and(|access| access.can_write()), cx);
            }
            e
        });
        // Reuse the live state, including unsaved edits and undo history.
        if let Some(peer) = self.document_peers(id, window, cx).iter().find(|peer| !peer.read(cx).preview) {
            let core = peer.read(cx).core.clone();
            editor.update(cx, |e, cx| e.receive_document(&core, cx));
        }
        // 阅读位置: back to where this document was left, on its first frame.
        let settings = self.store.settings();
        editor.update(cx, |e, _| {
            if let Some(selection) = settings.selections.get(id) {
                selection.restore(&mut e.core);
            }
            e.restore_top = self.store.scroll_top(id);
        });
        self.draft = None;
        self.subscribe_editor(&editor, window, cx);
        // Opening a document puts the caret in it, unless the user is typing a search.
        if !self.search_open {
            crate::gpui_compat::focus(window, &editor.read(cx).focus_handle(cx), cx);
        }
        editor.update(cx, |e, cx| { e.open_board_file(window, cx); e.open_table_file(window, cx); });
        if let Some(m) = self.store.touch(id)
            && let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                e.meta.opened = m.opened;
            }
        if let Some(previous) = &self.current
            && self.document_peers(previous, window, cx).is_empty() { self.store.forget_loaded(previous); }
        self.editor = Some(editor);
        self.current = Some(id.to_string());
        if self.store.set_active(id) { cx.defer(crate::settings::root_switched); }
        self.sidebar_entry = None;
        if let Some(e) = self.editor.clone() { e.update(cx, |e, cx| e.adopt_remote(cx)); }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            self.refresh_shared_entry(cx);
            if crate::cloud_sync::scope(self.store.root(), cx).is_some_and(|scope| scope.folder) {
                crate::cloud_sync::recheck_account(self.store.root(), cx);
            }
        }
        crate::quick_note::used(id, cx);
        if !cfg!(target_os = "macos") || !self.is_shared() { self.reveal_in_tree(id); }
        self.sync_sub_pages(cx);
        self.status = SaveStatus::Idle;
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        self.mode_menu = false;
        cx.notify();
        true
    }

    fn subscribe_editor(&mut self, editor: &Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_sub = Some(cx.subscribe_in(editor, window, |this, _, ev: &EditorEvent, window, cx| match ev {
            EditorEvent::Changed => {
                this.file_history.prefer_files = false;
                // 阅读模式 put the document back: nothing to save.
                let rejected = this.editor.clone().is_some_and(|ed| ed.update(cx, |e, cx| e.reject_read_only_edit(cx)));
                let composing = this.editor.as_ref().is_some_and(|ed| ed.read(cx).marked.is_some());
                if !rejected && !composing {
                    if this.draft.is_some() { this.unsaved = true; this.materialize_draft(cx); }
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
                if !this.flush(cx) { this.confirm = Some(Confirm::SaveFailed); cx.notify(); }
            }
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            EditorEvent::CloudUndo { redo } => {
                if this.flush(cx)
                    && let (Some(id), Some(editor)) = (this.current.clone(), this.editor.clone())
                {
                    let core = editor.read(cx).core.clone();
                    let peers: Vec<_> = this.document_peers(&id, window, cx).iter().filter(|ed| !ed.read(cx).preview)
                        .flat_map(|ed| { let s = ed.read(cx).core.sel; [s.anchor, s.head] }).collect();
                    match crate::cloud_sync::undo(&mut this.store, &id, &core, &peers, *redo, cx) {
                        Ok(Some((change, selection))) => {
                            let root = this.store.root().to_path_buf();
                            cx.defer(move |cx| apply_cloud_notes(&root, &[change], false, selection.map(|s| (editor.entity_id(), s)), cx));
                        }
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
            EditorEvent::OpenRelative(url) => this.open_relative(url, window, cx),
            EditorEvent::OpenDoc(id) => {
                #[cfg(target_os="ios")] this.mobile_follow_link(id,None,window,cx);
                #[cfg(not(target_os="ios"))] this.open(id,window,cx);
            }
            EditorEvent::OpenDocAt(id, ordinal) => {
                #[cfg(target_os="ios")] this.mobile_follow_link(id,Some(*ordinal),window,cx);
                #[cfg(not(target_os="ios"))] this.open_at(id,*ordinal,window,cx);
            }
            EditorEvent::HideRefs => this.ask_refs(false, cx),
            EditorEvent::HistoryEscape => {
                // Arrives from inside the editor's own update: leave the
                // preview (then the drawer) once that finishes.
                let this = cx.entity();
                cx.defer(move |cx| {
                    this.update(cx, |w, cx| {
                        if w.preview.is_some() { w.exit_preview(cx) } else { w.close_history(cx) }
                    });
                });
            }
            EditorEvent::Duplicate => {
                if let Some(id) = this.current.clone() {
                    this.duplicate_doc(&id, window, cx);
                }
            }
            EditorEvent::Leave => cx.defer_in(window, |this, window, cx| this.leave_board(window, cx)),
            EditorEvent::Catalogue(pinned) => {
                if let Some(id) = this.current.clone() {
                    if let Some(e) = this.entries.iter_mut().find(|e| e.id == id) {
                        e.meta.catalogue_closed = !pinned;
                    }
                    let _ = this.store.set_catalogue(&id, *pinned);
                }
            }
        }));
        #[cfg(target_os = "ios")]
        {
            let context = |ed: &Editor| (ed.mobile_table_target().is_some(),ed.mobile_block_toolbar_target().map(|(_,selected)|selected),ed.find.is_some(),ed.board_open());
            let mut editor_context = context(editor.read(cx));
            self.mobile_editor_context = Some(cx.observe(&editor,move |_,editor,cx| {
                let current = context(editor.read(cx));
                if current != editor_context { editor_context = current; cx.notify(); }
            }));
        }
    }

    pub(crate) fn start_draft(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        self.cancel_sidebar_open();
        self.close_history(cx);
        self.remember_scroll(cx);
        if let Some(id) = self.current.take() && self.document_peers(&id, window, cx).is_empty() { self.store.forget_loaded(&id); }
        let draft = crate::store::NewNoteDraft::new(parent);
        let editor = cx.new(|cx| {
            let mut e = Editor::new(draft.seed.clone(), crate::time::now(), Default::default(), window, cx);
            e.set_library_root(self.store.root().to_owned());
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            { e.cloud_history = !crate::cloud_restore::local_only(); }
            e
        });
        self.subscribe_editor(&editor, window, cx);
        self.draft = Some(draft);
        self.editor = Some(editor.clone());
        self.sync_links(cx);
        self.view = View::Doc;
        self.file_history.prefer_files = false;
        self.status = SaveStatus::Idle;
        self.search_open = false;
        self.search.update(cx, |s, cx| s.set_text("", cx));
        self.row_menu = None; self.doc_menu = false; self.doc_sub = None; self.doc_info = None; self.mode_menu = false;
        self.home.sort_menu = false;
        crate::gpui_compat::focus(window, &editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn drop_pristine_draft(&mut self, cx: &App) -> bool {
        let (Some(draft), Some(ed)) = (&self.draft, &self.editor) else { return false };
        let core = &ed.read(cx).core;
        if self.unsaved || !draft.untouched(core) { return false; }
        self.departing_focus = Some(ed.read(cx).focus.clone());
        self.draft = None; self.editor = None; self.editor_sub = None;
        true
    }

    fn materialize_draft(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(draft) = &self.draft else { return true; };
        let changes = self.store.track_file_changes();
        match self.store.with(None, |s| draft.materialize(s)) {
            Ok(id) => {
                self.draft = None;
                self.current = Some(id.clone());
                self.entries = self.store.list(); self.sort();
                self.remember_files(changes, crate::settings::tr("新建文档", "New document"));
                self.file_history.prefer_files = false;
                self.folded[1] = false;
                self.reveal_in_tree(&id); self.sync_sub_pages(cx);
                crate::quick_note::used(&id, cx);
                true
            }
            Err(e) => { self.status = SaveStatus::Failed(crate::settings::diagnostic(&e.to_string())); cx.notify(); false }
        }
    }

    /// ⌘N / the 文件 menu: create the 快捷新建 type (a fixed one, or the last
    /// one made from the 新建 menu) in the current place.
    pub fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self.creation_parent();
        self.create_doc(crate::settings::quick_new_markdown(), parent, window, cx);
    }

    /// Ask before 删除: a confirmation, then the 回收站.
    pub fn ask_delete(&mut self, id: &str, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let Some((room, title)) = self.shared_folder_context(id, cx) {
            if (id == room.scope.note && self.stopping_original(&room, cx))
                || (id != room.scope.note && self.shared_sidebar_folder_writable(&room, cx)) {
                self.confirm = Some(Confirm::DeleteSharedFolder { room, id: id.into(), title });
                cx.notify();
            }
            return;
        }
        self.confirm = Some(Confirm::Delete { ids: vec![self.row_of(id)], batch: false });
        cx.notify();
    }

    /// Move documents to the trash; if the open one goes, open the next.
    pub fn delete_docs(&mut self, ids: &[String], window: &mut Window, cx: &mut Context<Self>) {
        if self.store.file_task_busy() || self.file_history.busy || self.tree_writes_pending() { return; }
        // A root's row is never deleted this way: it leaves the sidebar (从侧栏移除).
        let ids: Vec<String> = ids.iter().filter(|id| crate::shared_store::root_of_row(id).is_none()).cloned().collect();
        let ids = &ids[..];
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
        let Some(task_guard) = self.store.begin_file_task() else { return };
        // A selection may span roots: one writer, and one write, per root.
        let mut writers: Vec<(String, crate::store::Store, Vec<(String, Option<String>)>)> = vec![];
        for g in groups {
            let Some(key) = self.store.root_of(&g.0) else { continue };
            match writers.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, v)) => v.push(g),
                None => if let Some(w) = self.store.root_writer(&key) { writers.push((key, w, vec![g])); },
            }
        }
        let task = cx.background_executor().spawn(async move {
            let (mut done, mut listed) = (DeletedDocs { deleted: vec![], errors: vec![], changes: vec![] }, vec![]);
            for (key, mut writer, groups) in writers {
                let journal = writer.track_file_changes();
                // One write for all of them: each index write waits for the disk.
                let (deleted, result) = writer.delete_many(&groups);
                done.deleted.extend(deleted);
                done.errors.extend(result.err().map(|e| crate::localized_format!("删除失败：{}", "Could not delete document: {}", crate::settings::diagnostic(&e.to_string()))));
                let warnings = writer.take_warnings();
                listed.push(ListedDocs { root: key, entries: writer.list(), trash: writer.list_trash(), warnings });
                done.changes.extend(journal.take());
            }
            (done, listed)
        });
        // The rows go now; the list read back when the files have moved
        // replaces this, bringing back anything that could not go.
        let all: std::collections::HashSet<String> = all.into_iter().collect();
        self.entries.retain(|e| !all.contains(&e.id));
        self.batch = None;
        self.row_menu = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let (done, lists) = task.await;
            drop(task_guard);
            let _ = this.update_in(cx, |w, window, cx| {
                let mut listed = ListedDocs { root: String::new(), entries: w.entries.clone(), trash: w.trash.clone(), warnings: vec![] };
                for l in lists {
                    listed.entries = w.store.merge_entries(&l.root, l.entries);
                    listed.trash = w.store.merge_trash(&l.root, l.trash);
                    listed.warnings.extend(l.warnings);
                }
                if w.current.as_ref().is_some_and(|c| done.deleted.contains(c)) {
                    w.save_task = None;
                    w.unsaved = false;
                    w.current = None;
                    w.editor = None;
                    w.editor_sub = None;
                }
                w.sync_peers(cx, &listed.entries, Some(&listed.trash));
                w.entries = listed.entries; w.trash = listed.trash;
                if w.folder_current.as_ref().is_some_and(|id| done.deleted.contains(id)) {
                    w.folder_current = None;
                    if w.view == View::Folder { w.view = View::Home; }
                }
                w.row_menu = None;
                if w.current.is_none() && w.view == View::Doc {
                    let next = crate::tree::Tree::new(&w.entries).rows(|id| w.expanded.contains(id)).into_iter()
                        .find(|(entry, _)| entry.meta.folder.is_none()).map(|(entry, _)| entry.id.clone());
                    match next {
                        Some(next) => {
                            let expanded = w.expanded.clone();
                            w.open(&next, window, cx); w.expanded = expanded;
                        }
                        None => w.view = View::Home,
                    }
                }
                #[cfg(target_os = "ios")]
                crate::ipad_notifications::publish(w,cx);
                w.remember_file_changes(done.changes, crate::settings::tr("删除", "Delete"));
                w.sync_sub_pages(cx);
                let mut errors = done.errors;
                errors.extend(listed.warnings.iter().map(|s| crate::settings::diagnostic(s)));
                if errors.is_empty() { w.show_toast(crate::settings::text("删除成功"), cx); }
                else {
                    let msg = errors.join("；");
                    w.show_toast_kind(ToastKind::Error, &msg, cx);
                    w.status = SaveStatus::Failed(msg);
                }
                cx.notify();
            });
        }).detach();
    }

    pub fn toggle_pin(&mut self, id: &str, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.toggle_shared_child_organization(id, true, cx) { return; }
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else { return };
        let on = !entry.meta.pinned;
        if let Err(error) = self.store.set_pinned(id, on) {
            self.show_toast_kind(crate::ui::ToastKind::Error, &crate::settings::diagnostic(&error.to_string()), cx);
            return;
        }
        entry.meta.pinned = on;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() { self.organization_changed(id, Some(on), None, cx); }
        self.sort();
        self.row_menu = None;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.organization_toast(true, on, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        self.show_toast(if on { crate::settings::text("已添加到“置顶”") } else { crate::settings::text("已从“置顶”移除") }, cx);
        cx.notify();
    }

    /// Favorites are listed on Home.
    pub fn toggle_star(&mut self, id: &str, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.toggle_shared_child_organization(id, false, cx) { return; }
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else { return };
        let on = entry.meta.starred == 0;
        let time = match self.store.set_starred(id, on) {
            Ok(time) => time,
            Err(error) => { self.show_toast_kind(crate::ui::ToastKind::Error, &crate::settings::diagnostic(&error.to_string()), cx); return; }
        };
        entry.meta.starred = time;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() { self.organization_changed(id, None, Some(time), cx); }
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.organization_toast(false, on, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        self.show_toast(if on { crate::settings::text("已添加到“收藏”") } else { crate::settings::text("已从“收藏”中移除") }, cx);
        cx.notify();
    }

    pub(crate) fn navigate_home(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() { self.navigate_personal(View::Home, None, _window, cx); return; }
        self.show_view(View::Home, cx);
    }

    pub(crate) fn breadcrumb_library_title(&self) -> &'static str {
        if self.is_shared() {
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            if self.shared_sidebar_section() == crate::sidebar::SidebarSection::Personal {
                return crate::settings::tr("我的文档", "My documents");
            }
            return crate::settings::tr("共享文档", "Shared documents");
        }
        crate::settings::tr("我的文档", "My documents")
    }

    pub(crate) fn navigate_breadcrumb_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() {
            #[cfg(target_os = "macos")]
            if self.shared_sidebar_section() == crate::sidebar::SidebarSection::Personal {
                self.navigate_personal_home_tab(crate::home::HomeTab::Recent, window, cx);
                return;
            }
            self.return_to_shared_list(window, cx);
            return;
        }
        self.navigate_home(window, cx);
    }

    /// 退出 on a standalone 画板: save it, close it, and show the folder it
    /// is in (主页 at the top).
    fn leave_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) { return; }
        let Some(id) = self.current.take() else { return };
        self.editor = None;
        self.editor_sub = None;
        if self.document_peers(&id, window, cx).is_empty() { self.store.forget_loaded(&id); }
        match self.entries.iter().find(|e| e.id == id).and_then(|e| e.meta.parent.clone()) {
            Some(p) => { self.open_folder(&p, window, cx); }
            None => self.show_view(View::Home, cx),
        }
        cx.notify();
    }

    /// Switch the main pane to 主页 or 回收站 (or back to the document).
    pub fn show_view(&mut self, view: View, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        self.cancel_sidebar_open();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.sharing.pending_open = None; self.sharing.account_open = None; self.conflict_review = None; }
        self.cancel_shortcut_recording(cx);
        self.close_history(cx);
        // 阅读位置: leaving the document with ⌘H or 回收站 also counts.
        if view != View::Doc {
            self.remember_scroll(cx);
            if self.draft.is_some() && !self.drop_pristine_draft(cx) && !self.flush(cx) { return; }
        }
        let view = if view == View::Doc && self.editor.is_none() { View::Home } else { view };
        if view == View::Trash {
            self.trash = self.store.list_trash();
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            self.close_sharing_for_navigation();
            if view != View::Settings { self.cloud_settings_from_share = false; }
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let view = if view == View::Shared { self.batch = None; self.home.tab = crate::home::HomeTab::Shared; View::Home } else { view };
        #[cfg(not(target_os = "ios"))]
        if matches!(view, View::Settings | View::Trash) && !matches!(self.view, View::Settings | View::Trash) {
            self.settings_return = self.view;
        }
        self.view = view;
        self.sidebar_entry = None;
        self.row_menu = None;
        self.trash_menu = None;
        self.theme_menu = None;
        self.lang_menu = None;
        self.pref_menu = None;
        self.close_font_menu(cx);
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

    pub(crate) fn render_toast(&self, window: &mut Window) -> Option<AnyElement> {
        let (kind, msg, t0) = self.toast.as_ref()?;
        crate::ui::toast_el(*kind, msg, *t0, window)
    }

    /// 重命名: the row's title turns into an input; Enter or clicking away saves, Esc cancels.
    pub fn start_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let section = self.is_shared().then(|| self.shared_sidebar_section());
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let section = None;
        self.start_rename_in_sidebar(id, section, window, cx);
    }

    pub(crate) fn start_rename_in_sidebar(&mut self, id: &str, section: Option<crate::sidebar::SidebarSection>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() {
            if !matches!(self.sharing_access(cx), Some(crate::cloud_sync::shared::Access::Owner | crate::cloud_sync::shared::Access::ReadWrite)) { return; }
            if section.is_none() { self.sharing.folded = false; }
        }
        if crate::shared_store::root_of_row(id).is_some() { return; }
        let Some(title) = self.entries.iter().find(|e| e.id == id).map(|e| e.title.trim().to_string()) else { return };
        let field = cx.new(|cx| {
            let mut f = TextField::new(crate::model::UNTITLED, cx);
            f.set_text(&title, cx);
            f.select_all_text(cx);
            f
        });
        let focus = field.focus_handle(cx);
        crate::gpui_compat::focus(window, &focus, cx);
        let subs = vec![
            cx.subscribe_in(&field, window, |w, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submit => w.finish_rename(true, window, cx),
                TextFieldEvent::Cancel => w.finish_rename(false, window, cx),
                _ => {}
            }),
            cx.on_blur(&focus, window, |w, window, cx| w.finish_rename(true, window, cx)),
        ];
        self.rename = Some(Rename { id: id.to_string(), sidebar_section: section, field, _subs: subs });
        // From 主页 the row may be hidden: show it in the sidebar.
        self.sidebar_open = true;
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let Some(section) = section { self.reveal_shared_sidebar_entry(&crate::sidebar::SidebarEntry::new(section, id), cx); }
        else { self.reveal_in_tree(id); }
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        self.reveal_in_tree(id);
        self.row_menu = None;
        cx.notify();
    }

    pub(crate) fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let commit = commit && (!self.is_shared() || matches!(self.sharing_access(cx), Some(crate::cloud_sync::shared::Access::Owner | crate::cloud_sync::shared::Access::ReadWrite)));
        let Some(r) = self.rename.take() else { return };
        let refocus = r.field.read(cx).is_focused(window);
        let title = r.field.read(cx).text.trim().to_string();
        let old = self.entries.iter().find(|e| e.id == r.id).map(|e| e.title.trim().to_string());
        // A document with children is renamed through its 正文, which renames the folder too.
        let r = Rename { id: self.note_or(&r.id), ..r };
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
                        // The folder of a 正文 was renamed with it: its path, and those inside, changed.
                        if self.entries.iter().any(|e| e.note.as_deref() == Some(r.id.as_str())) { self.entries = self.store.list(); }
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
                crate::gpui_compat::focus(window, &ed.read(cx).focus_handle(cx), cx);
            }
        self.sync_sub_pages(cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.publish_shared_folder_change(cx);
        cx.notify();
    }

    pub(crate) fn document_peers(&self, id: &str, window: &Window, cx: &mut Context<Self>) -> Vec<Entity<Editor>> {
        let root = self.store.root().to_path_buf();
        cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id())
            .filter_map(|w| w.downcast::<Workspace>())
            .filter_map(|w| w.update(cx, |ws, _, _| {
                (ws.store.root() == root && ws.current.as_deref() == Some(id)).then(|| ws.editor.clone()).flatten()
            }).ok().flatten()).collect()
    }

    /// 阅读位置: hand the open document's scroll offset to the library, so
    /// coming back to it lands where it was left.
    pub(crate) fn remember_scroll(&mut self, cx: &mut Context<Self>) {
        if self.preview.is_some() { return; }
        let (Some(id), Some(editor)) = (self.current.clone(), self.editor.clone()) else { return };
        if self.view != View::Doc { return; }
        let editor = editor.read(cx);
        let transient = !crate::FEI_NOTES || self.entries.iter().any(|e| e.id == id && e.meta.local());
        let selection = crate::editor::core::ReadingSelection::capture(&editor.core, transient);
        let _ = self.store.with(Some(&id), |s| s.remember_position(&id, selection, editor.scroll_top()));
        self.schedule_view_state(cx);
    }

    /// 最近打开 and 阅读位置 go to disk once documents stop changing for a
    /// moment: each write syncs fully, which made quick switching stall (#324).
    pub(crate) fn schedule_view_state(&mut self, cx: &mut Context<Self>) {
        self.view_state_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(600)).await;
            let _ = this.update(cx, |w, _| w.flush_view_state());
        }));
    }

    pub(crate) fn flush_view_state(&mut self) {
        self.view_state_task = None;
        if let Err(e) = self.store.flush_view_state() { crate::logging::write(&format!("Could not save the reading position: {e:#}")); }
    }

    pub(crate) fn sync_document(&mut self, window: &Window, cx: &mut Context<Self>) {
        // The previewed snapshot is not the live document: never hand it to a
        // sibling window.
        if self.preview.is_some() { return }
        let (Some(id), Some(editor)) = (self.current.clone(), self.editor.clone()) else { return };
        let peers = self.document_peers(&id, window, cx);
        if peers.is_empty() { return; }
        let core = editor.read(cx).core.clone();
        let root = self.store.root().to_path_buf();
        for handle in cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id()).filter_map(|w| w.downcast::<Workspace>()) {
            let _ = handle.update(cx, |ws, _, cx| {
                if ws.store.root() != root { return; }
                if let Some(entry) = ws.entries.iter_mut().find(|e| e.id == id) {
                    entry.title = core.doc.title.text.clone();
                    entry.icon = core.doc.icon.clone();
                }
                // A history window keeps showing its snapshot while its saved live state follows edits.
                if ws.current.as_deref() == Some(&id) {
                    if let Some(preview) = &mut ws.preview { preview.core = core.clone(); }
                    else if let Some(editor) = &ws.editor { editor.update(cx, |e, cx| e.receive_document(&core, cx)); }
                    ws.unsaved = true;
                }
                cx.notify();
            });
        }
    }

    fn schedule_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.is_some() { return }
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
                e.icon = ed.read(cx).core.doc.icon.clone();
                e.meta.modified = crate::time::now();
            }
        }
        self.sort();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.refresh_shared_entry(cx);
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            loop {
                match this.update(cx, |w, cx| w.editor.as_ref().is_some_and(|ed| ed.read(cx).marked.is_some())) {
                    Ok(true) => cx.background_executor().timer(SAVE_DELAY).await,
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            while this.update(cx, |w, _| w.store.file_task_busy()).unwrap_or(false) {
                cx.background_executor().timer(Duration::from_millis(50)).await;
            }
            let _ = this.update(cx, |w, cx| w.flush(cx));
        }));
        cx.notify();
    }

    pub(crate) fn visit_reminders(&self, cx: &App, mut visit: impl FnMut(&Entry,usize,&crate::model::InlineReminder)) {
        let open = self.editor.as_ref().map(|e| e.read(cx).core.doc.reminders());
        for entry in &self.entries {
            let reminders = match &open { Some(reminders) if self.current.as_deref() == Some(entry.id.as_str())=>reminders,_=>&entry.reminders };
            for (index,reminder) in reminders.iter().enumerate() { visit(entry,index,reminder); }
        }
    }

    #[cfg(not(target_os = "ios"))]
    fn check_reminders(&mut self, from: i64, to: i64, cx: &mut Context<Self>) {
        self.visit_reminders(cx,|entry,_,reminder| {
            if reminder.should_notify && from < reminder.notify && reminder.notify <= to {
                notify_system(&display_title(&entry.title),&crate::editor::reminder::notification(reminder));
            }
        });
    }

    /// The current window can already be in a GPUI input callback. update
    /// skips windows on the stack, whereas reading their root would panic.
    fn other_workspaces(cx: &mut Context<Self>) -> Vec<Entity<Self>> {
        let me = cx.entity_id();
        cx.windows().into_iter().filter_map(|h| h.update(cx, |root, _, _| root.downcast::<Self>().ok()).ok().flatten())
            .filter(|peer| peer.entity_id() != me).collect()
    }

    /// `on_stack`: the window being updated, if any (it can't be read now; it is this one).
    fn refresh_markdown(&mut self, on_stack: Option<gpui::WindowId>, cx: &mut Context<Self>) {
        if self.preview.is_some() || self.store.file_task_busy() || matches!(self.confirm, Some(Confirm::SaveFailed)) { return }
        let Some(id) = self.current.clone() else { return };
        if !self.store.is_markdown(&id) || self.store.file_path(&id).is_none()
            || self.editor.as_ref().is_some_and(|e| e.read(cx).marked.is_some()) { return }
        if self.unsaved {
            self.flush(cx);
            return
        }
        // A newly opened sibling can be clean while its source view still has a pending edit.
        if cx.windows().into_iter().filter(|h| Some(h.window_id()) != on_stack).filter_map(|h| h.downcast::<Workspace>()?.entity(cx).ok())
            .filter(|p| p.entity_id() != cx.entity_id())
            .any(|p| { let w = p.read(cx); w.store.root() == self.store.root() && w.current.as_ref() == Some(&id) && (w.unsaved || w.editor.as_ref().is_some_and(|e| e.read(cx).marked.is_some())) }) { return }
        if let Ok(Some(doc)) = self.store.with(Some(&id), |s| s.refresh_markdown(&id)) {
            self.adopt_markdown(&id, doc, cx);
        }
    }

    /// All live views share the store's save baseline, so install the accepted
    /// revision in each of them before a sibling's pending autosave can run.
    fn adopt_markdown(&mut self, id: &str, doc: crate::model::Document, cx: &mut Context<Self>) {
        let core = crate::editor::core::Core::new(doc);
        let install = |w: &mut Self, cx: &mut Context<Self>| {
            if let Some(p) = &mut w.preview { p.core = core.clone(); }
            else if let Some(ed) = &w.editor { ed.update(cx, |e, cx| e.receive_markdown(&core, cx)); }
            w.save_task = None;
            w.unsaved = false;
            w.dirty_since = None;
            w.status = SaveStatus::Saved;
            if let Some(entry) = w.entries.iter_mut().find(|e| e.id == id) {
                entry.title = core.doc.title.text.clone();
                entry.body = core.doc.plain_text().into();
                entry.reminders = core.doc.reminders();
                entry.links = core.doc.doc_links();
            }
            cx.notify();
        };
        install(self, cx);
        for peer in Self::other_workspaces(cx) {
            if peer.read(cx).store.root() == self.store.root() && peer.read(cx).current.as_deref() == Some(id) {
                peer.update(cx, install);
            }
        }
    }

    /// Write the open document to disk if it has unsaved changes. False when
    /// that failed: the changes stay pending (a later save retries them) and
    /// the top bar shows the error.
    pub fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        #[cfg(target_os = "ios")]
        crate::ipad_notifications::publish(self,cx);
        self.save_task = None;
        // 数据表 write their own files; a cell still being edited goes in first (leaving the page drops its view)
        if let Some(ed) = self.editor.clone() { ed.update(cx, |e, cx| e.commit_tables(cx)); }
        crate::table::flush_all(cx);
        // 版本历史 preview: the editor holds an older snapshot, so writing it
        // would overwrite the live document with history.
        if self.preview.is_some() { return true }
        if let Some(draft) = &self.draft && let Some(ed) = &self.editor {
            let core = &ed.read(cx).core;
            self.unsaved |= !draft.untouched(core);
            if self.unsaved && !self.materialize_draft(cx) { return false; }
        }
        if !self.unsaved {
            return true;
        }
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else {
            self.unsaved = false;
            return true;
        };
        let doc = &ed.read(cx).core.doc;
        let (title, icon, body, reminders, links) = (doc.title.text.clone(), doc.icon.clone(), doc.plain_text(), doc.reminders(), doc.doc_links());
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let core = ed.read(cx).core.clone();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let result = crate::cloud_sync::save(&mut self.store, &id, &core, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let result = self.store.with(Some(&id), |s| s.save_editor(&id, doc)).map(|meta| (meta, false));
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
                // A mirrored edit was saved by this window. A sibling must not
                // later write it a second time over a newer external edit.
                if meta.markdown && self.store.file_path(&id).is_some() {
                    for peer in Self::other_workspaces(cx) {
                        let w = peer.read(cx);
                        if w.store.root() == self.store.root() && w.unsaved && w.current.as_ref() == Some(&id) && w.preview.is_none()
                            && w.editor.as_ref().is_some_and(|e| e.read(cx).core.doc == ed.read(cx).core.doc) {
                            peer.update(cx, |w, cx| {
                                w.unsaved = false;
                                w.dirty_since = None;
                                w.save_task = None;
                                w.status = SaveStatus::Saved;
                                cx.notify();
                            });
                        }
                    }
                }
                self.sync_links(cx);
                // a renamed 数据表's file moved with its title
                let moved = (meta.kind == crate::store::DocKind::Table).then(|| self.store.file_path(&id)).flatten();
                ed.update(cx, |e, cx| {
                    e.modified = meta.modified;
                    if let Some(p) = &moved { e.table_file_at(p, cx); }
                    cx.notify();
                });
                self.status = SaveStatus::Saved;
                self.unsaved = false;
                self.sort();
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                self.refresh_shared_entry(cx);
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if cx.windows().len() > 1 {
                    let root = self.store.root().to_path_buf();
                    cx.defer(move |cx| cloud_saved_mirrors(&root, &id, &core.doc, cx));
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
        #[cfg(target_os = "ios")]
        crate::ipad_notifications::publish(self,cx);
        cx.notify();
        !self.unsaved
    }

    /// Save the open document before something replaces it. When the save
    /// fails nothing is replaced: the 保存失败 dialog offers 重试 / 放弃修改
    /// and runs `then` after either.
    /// 改用文件夹: move the library to real folders, then reopen the main
    /// window on it at the same place (documents get new ids in the new layout).
    pub(crate) fn migrate_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-restore")) { return; }
        if !self.save_before(cx, |w, window, cx| w.migrate_library(window, cx)) { return }
        let root = crate::store::Store::root_dir();
        let moved = crate::store::Store::migrate(&root).and_then(|r| Ok((r, crate::store::Store::open_at(root.clone())?)));
        match moved {
            Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("没能改为真实文件夹，文档库保持原样：{}", "Couldn't move to real folders; the library is unchanged: {}", crate::settings::diagnostic(&e.to_string())), cx),
            Ok((report, store)) => {
                let _ = self.store.with(None, |s| { *s = store; Ok(()) });
                crate::changelog::migrated(&report);
                crate::open_main_window(crate::devtools::scripted(), Some(window.bounds()), cx);
                window.remove_window();
            }
        }
    }

    pub fn save_before(&mut self, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> bool {
        if self.store.file_task_busy() { return false; }
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
        } else if self.draft.is_some() {
            self.draft = None; self.editor = None; self.editor_sub = None; self.save_task = None;
            self.unsaved = false; self.dirty_since = None; self.status = SaveStatus::Idle; self.view = View::Home;
        } else {
            let Some(id) = self.current.clone() else { return };
            match self.store.load_for_edit(&id, false) {
                Ok(doc) => {
                    if self.store.is_markdown(&id) { self.adopt_markdown(&id, doc, cx); }
                    else {
                        if let Some(ed) = &self.editor {
                            ed.update(cx, |e, cx| e.receive_document(&crate::editor::core::Core::new(doc), cx));
                        }
                        self.sync_document(window, cx);
                    }
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
        let Some(ed) = self.editor.clone() else { return };
        let id = self.current.clone().unwrap_or_default();
        let docs = self
            .entries
            .iter()
            // A folder with a 正文 is mentioned as that document (#357).
            .filter(|e| e.note.is_none())
            .map(|e| crate::editor::links::DocRef { id: e.id.clone(), title: e.title.clone(), icon: e.icon.clone(), folder: e.meta.folder.is_some(), modified: e.meta.modified, opened: e.meta.opened })
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
    pub(crate) fn open_at(&mut self, id: &str, ordinal: usize, window: &mut Window, cx: &mut Context<Self>) {
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
    /// traffic lights and 展开侧栏) on the left, … 导入 + on the right. Below a
    /// usable width it is None and the caller shows a search icon instead.
    pub(crate) fn header_search(&self, window: &Window, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let main = f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. };
        let title = crate::settings::label(crate::settings::text("主页"));
        let title_w = f32::from(window.text_system().shape_line(title.clone().into(), px(17.), &[window.text_style().highlight(HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() }).to_run(title.len())], None).width);
        // page_header's left inset (26 with the sidebar, else the traffic
        // lights plus the 4 + 28 + 12 of 展开侧栏), then 16 clear of the title.
        let left = if self.sidebar_open { 26. } else { theme::TRAFFIC_LIGHTS_W + 44. + if cfg!(target_os = "windows") { 32. } else { 0. } } + title_w + 16.;
        // … 导入 + are 32 wide each, then the right inset; 16 clear of them.
        let right = 3. * HEADER_ACTIONS_H + crate::ui::header_right_inset() + 16.;
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

    pub(crate) fn header_action_labels(&self, window: &Window) -> bool {
        let sidebar_w = if self.sidebar_open { self.sidebar_w(window) } else { 0. };
        f32::from(window.viewport_size().width) - sidebar_w >= 500.
    }

    /// `pane_w`: the width the header spans, to fold the breadcrumbs when narrow.
    fn top_bar(&self, pane_w: f32, _window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut title = self
            .editor
            .as_ref()
            .map(|e| display_title(&e.read(cx).core.doc.title.text))
            .unwrap_or_default();
        // a 画板 block open in the document's place: the document becomes a
        // crumb back to it, and the board is the page
        let board_block = self.editor.as_ref().is_some_and(|e| e.read(cx).board_block_open());
        let table_block = self.editor.as_ref().is_some_and(|e| e.read(cx).table_block_open());
        let table_file = self.editor.as_ref().is_some_and(|e| e.read(cx).table_file);
        let save_error = match &self.status {
            SaveStatus::Failed(error) => Some(error.clone()),
            _ => None,
        };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let collaboration_paused = crate::cloud_sync::shared::editing_paused(&self.store, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let collaboration_paused = false;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let sharing = self.sharing_toolbar(_window, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let sharing = None::<Div>;
        // Independent icon controls, with equal top and right insets.
        let seg = crate::ui::header_icon;
        let divider = || div().flex_none().w(px(1.)).h(px(18.)).bg(Theme::line_divider());
        // Breadcrumbs: the parent pages, each linking to itself, with the
        // middle ones folded into "…" when the chain is long.
        let ancestors = if let Some(id) = &self.current { self.ancestors(id) }
            else if let Some(parent) = self.draft.as_ref().and_then(|d| d.parent.as_ref()) {
                let mut path = self.ancestors(parent);
                path.extend(self.entries.iter().find(|e| &e.id == parent)); path
            } else { vec![] };
        let chain: Vec<_> = ancestors.iter().map(|e| (e.id.clone(), display_title(&e.title), e.icon.clone())).collect();
        let mut crumbs: Vec<AnyElement> = vec![];
        let sep = || div().mx(px(3.)).flex_none().child(icon("chevron-right", 12., Theme::icon())).into_any_element();
        // Every path starts at the library, as a folder page's does.
        if self.current.is_some() || self.draft.is_some() {
            crumbs.push(div().id("crumb-library").no_drag().flex_none().px(px(4.)).rounded(px(4.)).cursor_pointer()
                .whitespace_nowrap().text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::primary_500()))
                .child(ui_text(self.breadcrumb_library_title().to_string()))
                .on_click(cx.listener(|w, _, window, cx| w.navigate_breadcrumb_library(window, cx)))
                .into_any_element());
            crumbs.push(sep());
        }
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
        if board_block {
            crumbs.push(
                div().id("crumb-board-doc").no_drag().min_w(px(28.)).max_w(px(200.)).px(px(4.)).flex().items_center().rounded(px(4.)).cursor_pointer()
                    .text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::primary_500()))
                    .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title.clone())))
                    .on_click(cx.listener(|w, _, window, cx| if let Some(e) = w.editor.clone() { e.update(cx, |e, cx| e.close_open_board(window, cx)) }))
                    .into_any_element(),
            );
            crumbs.push(sep());
            title = crate::settings::tr("画板", "Board").to_string();
        }
        if table_block {
            crumbs.push(
                div().id("crumb-table-doc").no_drag().min_w(px(28.)).max_w(px(200.)).px(px(4.)).flex().items_center().rounded(px(4.)).cursor_pointer()
                    .text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::primary_500()))
                    .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title.clone())))
                    .on_click(cx.listener(|w, _, window, cx| if let Some(e) = w.editor.clone() { e.update(cx, |e, cx| e.close_open_table(window, cx)) }))
                    .into_any_element(),
            );
            crumbs.push(sep());
            title = crate::settings::tr("数据表", "Data table").to_string();
        }
        // Too narrow for the path: it folds into one "…" (the path on hover),
        // and the title keeps the room. Left: the sidebar's buttons when it is
        // closed; right: the three actions.
        let left = if self.sidebar_open { 18. } else { theme::TRAFFIC_LIGHTS_W + 4. + 28. + 4. + 28. + 25. };
        let room = pane_w - left - crate::ui::header_right_inset() - 3. * HEADER_ACTIONS_H - 8. - 16.;
        if room < 360. && !crumbs.is_empty() {
            let path: Vec<&str> = std::iter::once(self.breadcrumb_library_title()).chain(chain.iter().map(|c| c.1.as_str())).collect();
            crumbs = vec![
                div().id("crumb-more").no_drag().flex_none().size(px(20.)).flex().items_center().justify_center().rounded(px(4.))
                    .text_color(Theme::text_caption()).tip(&path.join(" / "), None).child("…").into_any_element(),
                sep(),
            ];
        }
        let record_more = self.more_btn_bounds.clone();
        let doc_open = self.doc_menu;
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
                        .on_click(cx.listener(|w, _, window, cx| w.navigate_home(window, cx))),
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
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .children(crumbs)
                            .child(
                                div()
                                    .id("top-title")
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .line_height(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Theme::text())
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(ui_text(title))
                                    // a 数据表 file has no title in its page: double-click its name here to rename it
                                    .when(table_file, |d| d.no_drag().on_click(cx.listener(|w, ev: &ClickEvent, window, cx| if ev.click_count() >= 2 && let Some(id) = w.current.clone() { w.start_rename(&id, window, cx); }))),
                            ),

                    )
                    .when_some(save_error, |d, error| d.child(
                        div().id("save-status-error").when(self.unsaved, |d| d.cursor_pointer().on_click(cx.listener(|w, _, _, cx| { w.confirm = Some(Confirm::SaveFailed); cx.notify(); }))).min_w_0().max_w(px(220.)).text_size(px(12.)).line_height(px(18.))
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
                    .child(crate::ui::toolbar_group(div().flex_none().flex().items_center().gap(px(4.)))
                    .children(self.document_mode_button(_window, cx))
                    .children(sharing)
                    .when(self.current.is_some(), |d| {
                        d.child(
                            // No tooltip while its dropdown is open; the menu hangs
                            // from this button's right edge.
                            crate::ui::grouped_header_icon("doc-more", "ellipsis")
                                .relative()
                                .when(self.doc_menu, |d| d.bg(crate::ui::toolbar_color("toolbar.button.hover").unwrap_or_else(Theme::fill_hover)))
                                .when(!self.doc_menu, |d| d.tip(crate::settings::tr("更多", "More"), None))
                                .child(canvas(move |b, _, _| record_more.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                                // Toggle on mouse-down from the render-time state: the menu's outside-press
                                // handler has already cleared `doc_menu` by the time a click would fire.
                                .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, _, cx| {
                                    cx.stop_propagation();
                                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                                    if w.is_shared() { w.close_sharing_for_navigation(); }
                                    w.doc_menu = !doc_open;
                                    w.doc_sub = None;
                                    cx.notify();
                                })),
                        )
                    }))
                    .child(
                        seg("doc-search", "search")
                            .tip_label(crate::shortcuts::keys(crate::settings::text("搜索(⌘ + J)")).into(), TipSide::Below)
                            .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(FocusSearch), cx))),
                    )
                    .into_any_element()
            )
    }

    pub(crate) fn render_shared_waiting(&self, cx: &mut Context<Self>) -> AnyElement {
        let revoked = {
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            { crate::cloud_sync::shared::access(&self.store, cx) == Some(crate::cloud_sync::shared::Access::Revoked) }
            #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
            { let _ = cx; false }
        };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if crate::cloud_sync::shared::editing_paused(&self.store, cx) && !revoked {
            return div().size_full().flex().items_center().justify_center().text_color(Theme::text_caption())
                .child(crate::cloud_trial::access_required()).into_any_element();
        }
        let message = if revoked { crate::settings::tr("共享访问已结束", "Access to this shared document has ended") }
            else { crate::settings::tr("正在接收共享文档", "Waiting for the shared document") };
        let body = div().flex_1().flex().flex_col().items_center().justify_center().gap(px(12.))
            .text_color(Theme::text_caption()).child(message);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let body = body
            .when(crate::cloud_sync::shared::editing_paused(&self.store, cx) && !revoked, |d| d.child(
                div().text_size(px(13.)).child(crate::settings::tr("协作已暂停。请检查 iCloud 账户和云功能状态。", "Collaboration is paused. Check your iCloud account and cloud access."))))
            .child(crate::settings::button("shared-return-list", crate::settings::tr("返回共享列表", "Back to shared documents"))
                .on_click(cx.listener(|w, _, window, cx| w.return_to_shared_list(window, cx))));
        div().size_full().flex().flex_col()
            .child(crate::trash::page_header(self, crate::settings::tr("共享文档", "Shared Document"), 26., cx))
            .child(body).into_any_element()
    }
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_open_document_ids(root: &std::path::Path, cx: &mut App) -> Vec<String> {
    cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()).filter_map(|window|
        window.update(cx, |ws, _, _| (ws.store.root() == root && ws.editor.is_some())
            .then(|| ws.current.clone()).flatten()).ok().flatten()).collect()
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_open_positions(root: &std::path::Path, cx: &mut App) -> Option<HashMap<String, Vec<crate::editor::core::Pos>>> {
    let mut positions: HashMap<_, Vec<_>> = HashMap::new();
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let selection = window.update(cx, |ws, _, cx| {
            if ws.store.root() != root { return Some(None); }
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

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn cloud_saved_mirrors(root: &std::path::Path, id: &str, doc: &crate::model::Document, cx: &mut App) {
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, _, cx| {
            if ws.store.root() == root && ws.unsaved && ws.current.as_deref() == Some(id)
                && ws.preview.as_ref().map_or_else(|| ws.editor.as_ref().is_some_and(|editor| editor.read(cx).core.doc == *doc), |p| p.core.doc == *doc)
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

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_active_selection(root: &std::path::Path, cx: &mut App) -> crate::cloud_sync::presence::SelectionSource {
    use crate::cloud_sync::presence::SelectionSource::*;
    #[cfg(target_os = "ios")]
    let window = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>());
    #[cfg(target_os = "macos")]
    let window = cx.active_window().and_then(|window| window.downcast::<Workspace>());
    let Some(window) = (if crate::devtools::scripted() {
        cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>())
    } else { window }) else { return Idle };
    window.update(cx, |ws, window, cx| {
        if ws.store.root() != root { return Idle; }
        let (Some(id), Some(editor)) = (&ws.current, &ws.editor) else { return Idle };
        let editor = editor.read(cx);
        if ws.view != View::Doc || ws.history.is_some() || ws.preview.is_some() || !editor.cloud_history || editor.read_only { return Idle; }
        #[cfg(target_os = "macos")]
        if !crate::devtools::scripted() && (!window.is_window_active() || !editor.focus.is_focused(window)) { return Idle; }
        #[cfg(target_os = "ios")]
        { let _ = window; if !crate::ipad::editor_presence_active(ws) { return Idle; } }
        let selection = (!ws.unsaved && !editor.composing() && !editor.core.in_block_mode() && editor.core.cell_sel.is_none())
            .then_some(editor.core.sel);
        Editing(id.clone(), selection)
    }).unwrap_or(Idle)
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_account_notice(root: &std::path::Path, phase: crate::cloud_sync::SyncPhase, cx: &mut App) {
    let message = if phase == crate::cloud_sync::SyncPhase::SignedOut {
        crate::settings::tr("iCloud 已退出。请在文档库设置中继续。",
            "You’re signed out of iCloud. Continue in Documents settings.")
    } else {
        crate::settings::tr("iCloud 账户已切换。请在文档库设置中继续。",
            "The iCloud account changed. Continue in Documents settings.")
    };
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, _, cx| { if ws.store.root() == root { ws.show_toast_kind(ToastKind::Info, message, cx); } });
    }
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_attachments_ready(root: &std::path::Path, cx: &mut App) {
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, _, cx| {
            if ws.store.root() == root && let Some(editor) = &ws.editor {
                editor.update(cx, |editor, cx| editor.cloud_attachments_ready(cx));
            }
        });
    }
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn cloud_notes_changed(root: &std::path::Path, notes: &[crate::cloud_sync::NoteChange], remote: bool, cx: &mut App) {
    apply_cloud_notes(root, notes, remote, None, cx)
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn apply_cloud_notes(root: &std::path::Path, notes: &[crate::cloud_sync::NoteChange], remote: bool,
    origin: Option<(EntityId, crate::editor::core::Selection)>, cx: &mut App) {
    if root == crate::store::Store::root_dir() { crate::sharing::organizer::reload(cx); }
    for window in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
        let _ = window.update(cx, |ws, window, cx| {
            if ws.store.root() != root { return; }
            // Merge into the live editor, then put the selected snapshot back on screen.
            let preview = ws.preview.as_ref().map(|p| p.name.clone());
            if preview.is_some() { ws.exit_preview(cx); }
            ws.entries = ws.store.list();
            ws.sort();
            let access = crate::cloud_sync::shared::access(&ws.store, cx);
            let access_paused = crate::cloud_sync::shared::editing_paused(&ws.store, cx);
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
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                { ws.sharing.panel_open = false; }
                ws.editor = None;
                ws.editor_sub = None;
                ws.save_task = None;
                if ws.view == View::Doc { ws.view = View::Home; }
            }
            // An iPad can show a personal folder alongside a standalone
            // shared editor. A refresh of the share cannot remove that folder.
            #[cfg(target_os = "ios")]
            let folder_is_in_updated_library = !ws.mobile_personal_browser(cx);
            #[cfg(not(target_os = "ios"))]
            let folder_is_in_updated_library = true;
            if folder_is_in_updated_library && ws.folder_current.as_ref().is_some_and(|id| !ws.entries.iter().any(|e| &e.id == id)) {
                ws.folder_current = None;
                if ws.view == View::Folder { ws.view = View::Home; }
            }
            if ws.view == View::Trash { ws.trash = ws.store.list_trash(); }
            if let Some(room) = crate::cloud_sync::shared::scope(&ws.store, cx).cloned() {
                #[cfg(target_os = "ios")]
                if ws.sharing.opening.is_some() {
                    if ws.view == View::Home { ws.complete_shared_open(window, cx); }
                } else if ws.current.is_none() && ws.view != View::Folder && ws.entries.iter().any(|entry| entry.id == room.note) {
                    ws.open(&room.note, window, cx);
                }
                #[cfg(target_os = "macos")]
                { let _ = room; ws.finish_mac_shared_waiting(window, cx); }
                let reading = ws.reading();
                if let Some(editor) = &ws.editor {
                    editor.update(cx, |editor, cx| {
                        editor.set_shared_access(reading, access_paused, access.is_some_and(|access| access.can_write()), cx);
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
                    if let Some((_, saved)) = origin.filter(|(id, _)| *id == cx.entity_id()) {
                        selection = saved;
                        editor.autoscroll = true;
                    }
                    editor.receive_document_at(&crate::editor::core::Core::new(change.document.clone()), selection, cx);
                    if let Some(modified) = modified { editor.modified = modified; }
                });
                if !remote {
                    ws.unsaved = false;
                    ws.save_task = None;
                    ws.dirty_since = None;
                    ws.status = SaveStatus::Saved;
                }
            }
            if let Some(name) = preview {
                if ws.current.as_ref() == ws.history.as_ref().map(|h| &h.id) { ws.preview_version(&name, window, cx); }
                else { ws.close_history(cx); }
            }
            ws.sync_links(cx);
            ws.refresh_shared_entry(cx);
            cx.notify();
        });
    }
}

/// Header and action group share an 8px top/right inset.
pub const HEADER_H: f32 = 48.;
/// The narrowest the main pane (document, Home, settings) gets and the
/// window's minimum width: a narrower window hides the sidebar.
pub const DOC_MIN_W: f32 = 480.;
pub(crate) const HEADER_ACTIONS_H: f32 = 32.;

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar_window_active = window.is_window_active();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.view == View::Doc && self.preview.is_none() && self.is_shared() && window.is_window_active()
            && let Some(editor) = &self.editor
            && let Ok(displayed) = crate::cloud_sync::displayed_shared_updates(self.store.root(), &editor.read(cx).core.doc, cx)
            && !displayed.is_empty()
        {
            let root = self.store.root().to_path_buf();
            let workspace = cx.weak_entity();
            window.on_next_frame(move |window, cx| {
                let visible = workspace.update(cx, |ws, cx| ws.store.root() == root && ws.view == View::Doc && ws.preview.is_none()
                    && ws.editor.as_ref().is_some_and(|editor| crate::cloud_sync::displayed_shared_updates(&root, &editor.read(cx).core.doc, cx)
                        .is_ok_and(|current| displayed.iter().all(|id| current.contains(id))))).unwrap_or(false);
                if window.is_window_active() && visible
                    && let Err(error) = crate::cloud_sync::mark_shared_updates_read(&root, &displayed, cx)
                { crate::logging::write(&format!("Shared note read state: {error:#}")); }
            });
        }
        self.sync_note_rows();
        // The menu bar is the front window's: a Markdown file's 插入 / 格式 leave out what it can't hold.
        if !cfg!(target_os = "ios") && window.is_window_active() {
            let markdown = self.view == View::Doc && self.editor.as_ref().is_some_and(|e| e.read(cx).markdown);
            cx.defer(move |cx| menus_follow(markdown, cx));
        }
        // Clicks no longer take focus from the editor (see the root's mouse
        // down), so leaving the document hands it back to the window's keys.
        let departed = self.departing_focus.take();
        if self.view != View::Doc && (departed.is_some_and(|f| f.is_focused(window) || window.focused(cx).is_none())
            || self.editor.as_ref().is_some_and(|e| e.read(cx).focus.is_focused(window))) { crate::gpui_compat::focus(window, &self.focus, cx); }
        #[cfg(not(target_os = "ios"))]
        self.sync_confirm_focus(window, cx);
        crate::editor::image::frame_drawn(window, cx);
        #[cfg(target_os = "ios")]
        if crate::mobile::native_shell(window) { return self.render_mobile(window, cx); }
        #[cfg(not(target_os = "ios"))]
        {
        // Full screen: the sidebar is solid (`Theme::window`) and the native glass stays hidden.
        crate::theme::set_fullscreen(window.is_fullscreen());
        self.native_sidebar.sync(if self.sidebar_open && !crate::theme::fullscreen() { self.sidebar_w(window) } else { 0. }, window);
        // The pane keeps its minimum beside the sidebar: opening the sidebar in
        // a narrow window widens the window rather than squeezing the note, and
        // narrowing the window below that (half-screen snapping) hides the
        // sidebar instead of being refused. The window's own minimum is DOC_MIN_W.
        let vs = window.viewport_size();
        let vw = f32::from(vs.width);
        if self.widen_from.is_some_and(|w| (w - vw).abs() > 0.5) { self.widen_from = None; }
        // The web build's window is the page: never resized from here.
        // Opened by hand: no longer waiting to come back by itself.
        if self.sidebar_open { self.sidebar_auto_hidden = false; }
        if !window.is_fullscreen() && !cfg!(target_family = "wasm") {
            let need = DOC_MIN_W + self.sidebar_w(window);
            if self.sidebar_open && vw < need - 0.5 {
                if !self.sidebar_was_open { window.resize(size(px(need), vs.height)); self.widen_from = Some(vw); }
                else if self.widen_from.is_none() { self.sidebar_open = false; self.sidebar_auto_hidden = true; }
            } else if self.sidebar_auto_hidden && vw >= need - 0.5 {
                self.sidebar_open = true;
                self.sidebar_auto_hidden = false;
            }
        }
        self.sidebar_was_open = self.sidebar_open;
        }
        let sidebar = if self.sidebar_open { Some(if matches!(self.view, View::Settings | View::Trash) { self.render_settings_sidebar(window, cx) } else { self.render_sidebar(window, cx) }) } else { None };
        let row_menu = self.render_row_menu(window, cx);
        let resize = self.sidebar_open.then(|| self.render_resize_handle(window, cx));
        let resize_overlay = self.render_resize_overlay(cx);
        let folder_drag_overlay = self.render_folder_drag_overlay(cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let shared_drag_overlay = self.with_sidebar(true, |w| w.render_folder_drag_overlay(cx));
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let shared_drag_overlay = None::<AnyElement>;
        let home_drag_overlay = self.render_home_drag_overlay(window, cx);
        let file_drop = self.render_file_drop(window, cx);
        let create_menu = self.render_create_menu(cx);
        let import_menu = self.render_import_menu(cx);
        let place_menu = self.render_place_menu(cx);
        let search = self.render_search(window, cx);
        let doc_menu = self.render_doc_menu(window, cx);
        let doc_info = self.render_doc_info(cx);
        let history = self.render_history(cx);
        let confirm = self.render_confirm(window, cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let conflict_review = self.render_conflict_review(window, cx);
        let trash_menu = self.render_trash_menu(cx);
        let home_menus = self.render_home_menus(cx);
        let theme_menu = self.render_theme_menu(cx);
        let lang_menu = self.render_lang_menu(cx);
        let pref_menu = self.render_pref_menu(cx);
        let font_menu = self.render_font_menu(cx);
        let toast = self.render_toast(window);
        let forced_update = crate::update::prompt(window, cx);
        let mode_menu = self.render_mode_menu(cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let sharing_panel = self.render_sharing_toolbar_panel(window, cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let sharing_sheet = self.render_sharing_sheet(window, cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let sharing_sheet = None::<AnyElement>;
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let sharing_panel = None::<AnyElement>;
        let main = match self.view {
            View::Doc => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(self.top_bar(f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. }, window, cx))
                .children(self.render_history_banner(cx))
                .map(|d| {
                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                    let d = d.children(self.render_conflict_banner(cx));
                    d
                })
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
            View::Home if self.is_shared() => self.render_shared_waiting(cx),
            View::Home => self.render_home(window, cx),
            View::Folder => self.render_folder(window, cx),
            View::Trash => self.render_trash(cx),
            View::Settings => self.render_settings(window, cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            View::Shared => self.render_shared_list(window, cx),
        };
        let move_dlg = self.render_move_dialog(cx);
        let export_md = self.render_export_md(cx);
        // Only the note stands out: a white plane flush with the window's top,
        // right and bottom edges. The sidebar sits sunken on the window
        // surface beside it; a faint shade along the plane's left edge parts
        // the two (no gutter, radius or border).
        // A document sits on its theme's page colour.
        let page = if self.view == View::Doc { crate::editor_theme::Theme::bg_body() } else { Theme::bg_body() };
        let note = div().id("note").relative().flex_1().min_w_0().h_full().flex().bg(page).when(!crate::FEI_NOTES, |d| d.shadow(theme::shadow_note())).child(main)
            .when(cfg!(target_os = "macos") && crate::FEI_NOTES, |d| d.overflow_hidden())
            // Files from the Finder dropped on 主页 or a folder's page are imported there.
            .when(!self.is_shared() && matches!(self.view, View::Home | View::Folder), |d| d
                .on_drag_move(cx.listener(|w, ev: &DragMoveEvent<ExternalPaths>, _, cx| w.file_drag_move(ev, cx)))
                .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| w.file_dropped(paths, window, cx))))
            .children(file_drop)
            .children(history);
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(|w, ev: &MouseDownEvent, window, cx| {
                if ev.button == MouseButton::Left && ev.position.y >= px(HEADER_H) {
                    let sidebar = w.sidebar_open && ev.position.x < px(w.sidebar_w(window));
                    if w.sidebar_focused != sidebar { w.sidebar_focused = sidebar; cx.notify(); }
                }
            }))
            .capture_key_down(cx.listener(|w, ev: &KeyDownEvent, window, cx| {
                if w.sidebar_focused && !ev.keystroke.modifiers.platform && !ev.keystroke.modifiers.control
                    && w.view == View::Doc && w.editor.as_ref().is_some_and(|e| e.read(cx).focus.is_focused(window)) {
                    w.sidebar_focused = false; cx.notify();
                }
            }))
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
            .font_family(theme::ui_family())
            .text_color(Theme::text())
            // Window keys act on the window they were pressed in, not on
            // whichever window macOS considers active.
            .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, window, cx| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                w.with_sidebar(true, |w| w.track_sidebar_press(ev, window, cx));
                w.track_sidebar_press(ev, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|w, _, window, cx| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if w.sharing.sidebar.active.is_some() { w.finish_shared_sidebar_click(window, cx); return; }
                w.finish_sidebar_click(window, cx);
            }))
            .map(|d| self.workspace_actions(d,cx))
            .children(sidebar)
            .child(note)
            .children(resize)
            .children(crate::ui::window_controls(window))
            .children(row_menu)
            .children(doc_menu)
            .children(sharing_panel)
            .children(sharing_sheet)
            .children(mode_menu)
            .children(doc_info)
            .children(trash_menu)
            .children(home_menus)
            .children(theme_menu)
            .children(lang_menu)
            .children(pref_menu)
            .children(font_menu)
            .children(confirm)
            .map(|d| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                let d = d.children(conflict_review);
                d
            })
            .children(move_dlg)
            .children(export_md)
            .children(search)
            .children(resize_overlay)
            .children(folder_drag_overlay).children(shared_drag_overlay)
            .when(self.file_history.busy || self.store.file_task_busy(), |d| d.child(deferred(div().id("file-history-pending").absolute().inset_0().occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())).with_priority(20)))
            .children(home_drag_overlay)
            .children(create_menu)
            .children(import_menu)
            .children(place_menu)
            .children(forced_update)
            .children(toast)
            // Over everything, toasts included, until its last page is done.
            .when_some(self.onboarding.clone(), |d, o| d.child(deferred(div().id("onboarding-cover").absolute().inset_0().occlude().child(o)).with_priority(30))).into_any_element()
    }
}

impl Workspace {
    /// Lay the first-launch pages over this window; finishing them uncovers it.
    pub(crate) fn replay_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-restore")) { return; }
        if self.onboarding.is_some() { return }
        let o = cx.new(|cx| crate::onboarding::Onboarding::replay(window, cx));
        cx.subscribe_in(&o, window, |w, _, _: &crate::onboarding::Done, window, cx| {
            w.onboarding = None;
            // The language may have changed on the first page.
            crate::settings::refresh_all(w, cx);
            crate::gpui_compat::focus(window,&w.focus,cx);
            cx.notify();
        }).detach();
        self.onboarding = Some(o);
        cx.notify();
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
                fdl_core::vfs::write(store.file_path(&id).unwrap(), crate::docxml::serialize(&doc(&format!("disk-{name}")))).unwrap();
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


impl Workspace {
    pub(crate) fn workspace_actions(&self, root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.capture_action(cx.listener(|w, _: &crate::editor::keymap::Undo, window, cx| w.capture_file_history(false,window,cx)))
            .capture_action(cx.listener(|w, _: &crate::editor::keymap::Redo, window, cx| w.capture_file_history(true,window,cx)))
            .on_action(cx.listener(|w, _: &CloseWindow, window, cx| {
                #[cfg(target_os = "ios")]
                w.mobile_back(window,cx);
                #[cfg(not(target_os = "ios"))]
                {
                    crate::quick_note::persist(cx);
                    if w.save_before(cx, |_, window, _| window.remove_window()) { window.remove_window(); }
                }
            }))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &Zoom, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(cx.listener(|w, _: &OpenSettings, window, cx| w.open_global_settings(window, cx)))
            .on_action(cx.listener(|w, _: &ReplayOnboarding, window, cx| w.replay_onboarding(window,cx)))
            .on_action(cx.listener(|w, _: &OpenAbout, _, cx| w.open_settings(crate::settings::Page::About, cx)))
            .on_action(cx.listener(|w, _: &CheckForUpdates, _, cx| { w.open_settings(crate::settings::Page::About, cx); crate::update::check(true, cx); }))
            .on_action(cx.listener(|w, _: &GoHome, _, cx| { if !w.is_shared() { w.show_view(View::Home, cx); } }))
            .on_action(cx.listener(|w, _: &NewDoc, window, cx| w.new_doc(window, cx)))
            .on_action(cx.listener(|w, _: &ImportDoc, window, cx| { if !w.is_shared() { w.open_import(window, cx); } }))
            .on_action(cx.listener(|w, _: &OpenFile, window, cx| { if !w.is_shared() { w.open_file_dialog(window, cx); } }))
            .on_action(cx.listener(|w, _: &OpenFolder, window, cx| { if !crate::FEI_NOTES && !w.is_shared() { w.open_folder_dialog(window, cx); } }))
            .on_action(cx.listener(|w, _: &ImportFolder, window, cx| { if !w.is_shared() { w.open_import_folder(window, cx); } }))
            .on_action(cx.listener(|w, _: &ToggleSidebar, _, cx| {
                if w.is_shared() { return; }
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
                if (w.theme_menu.is_some() && w.theme_menu_key(&ev.keystroke.key, cx)) || (w.lang_menu.is_some() && w.lang_menu_key(&ev.keystroke.key, cx)) || (w.pref_menu.is_some() && w.pref_menu_key(&ev.keystroke.key, cx)) {
                    cx.stop_propagation();
                    return;
                }
                // Esc leaves 设置 (unless its filter field has the caret).
                if ev.keystroke.key == "escape" && w.view == View::Settings && !w.settings_filter.read(cx).is_focused(window) {
                    #[cfg(target_os = "ios")]
                    w.mobile_back(window,cx);
                    #[cfg(not(target_os = "ios"))]
                    w.close_settings(cx);
                    cx.stop_propagation();
                    return;
                }
                if ev.keystroke.key == "escape" && (w.doc_info.is_some() || w.history.is_some() || w.doc_menu || w.mode_menu) {
                    w.doc_info = None;
                    if w.history.is_some() { w.close_history(cx); }
                    w.mode_menu = false;
                    w.doc_menu = false;
                    w.doc_sub = None;
                    cx.stop_propagation();
                    cx.notify();
                }
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if ev.keystroke.key == "escape" && (w.sharing.panel_open || w.sharing.sync_panel_open) {
                    w.sharing.panel_open = false;
                    w.sharing.sync_panel_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            // Esc leaves 批量操作 (closing its menu or confirmation first), even from the editor.
            .capture_key_down(cx.listener(|w, ev: &KeyDownEvent, window, cx| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if ev.keystroke.key == "escape" && w.sharing.sheet.is_some() {
                    w.close_sharing(window, cx); cx.stop_propagation(); return;
                }
                let undo = crate::gpui_compat::secondary(ev.keystroke.modifiers,cx) && matches!(ev.keystroke.key.as_str(), "z" | "y");
                if w.store.file_task_busy() || (w.file_history.busy && !undo) { cx.stop_propagation(); return; }
                if ev.keystroke.key == "escape" && w.preview.is_some() { w.exit_preview(cx); cx.stop_propagation(); return; }
                if ev.keystroke.key == "escape" && w.history.is_some() { w.close_history(cx); cx.stop_propagation(); return; }
                if ev.keystroke.key == "escape" && w.has_home_drag() {
                    w.cancel_home_drag(cx); cx.stop_propagation(); return;
                }
                if ev.keystroke.key == "escape" && w.folder_drag.active.is_some() {
                    w.end_folder_drag(true, window, cx); cx.stop_propagation(); return;
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
    }
}

#[cfg(test)]
mod menu_tests {
    use super::{Menu, MenuItem, menu_bar};
    use crate::settings::tr;

    fn names(menus: &[Menu]) -> Vec<String> {
        fn walk(items: &[MenuItem], out: &mut Vec<String>) {
            for i in items {
                match i {
                    MenuItem::Action { name, .. } => out.push(name.to_string()),
                    MenuItem::Submenu(m) => { out.push(m.name.to_string()); walk(&m.items, out); }
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        for m in menus { out.push(m.name.to_string()); walk(&m.items, &mut out); }
        out
    }

    #[test]
    fn markdown_menus_leave_out_what_markdown_cannot_keep() {
        let (doc, md) = (names(&menu_bar(false)), names(&menu_bar(true)));
        let gone = [tr("提醒", "Reminder"), tr("下划线", "Underline"), tr("对齐", "Align"), tr("左对齐", "Align left"), tr("居中对齐", "Align center"), tr("右对齐", "Align right")];
        for n in gone { assert!(doc.iter().any(|d| d == n) && !md.iter().any(|d| d == n), "{n}"); }
        // Everything else stays, in the same order.
        assert_eq!(doc.into_iter().filter(|d| !gone.contains(&d.as_str())).collect::<Vec<_>>(), md);
    }
}
