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

actions!(workspace, [NewDoc, OpenFile, OpenFolder, ImportDoc, ImportFolder, ImportFromApps, ToggleSidebar, FocusSearch, FindInDoc, ReplaceInDoc, OpenSettings, OpenAbout, CheckForUpdates, GoHome, CloseWindow, Quit,
    ReplayOnboarding, HideApp, HideOthers, ShowAll, Minimize, Zoom, ToggleFullScreen, ToggleDarkMode]);
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
    vec![
        Menu {
            name: "Quiet.md".into(),
            items: vec![
                MenuItem::action(tr("关于 Quiet.md", "About Quiet.md"), OpenAbout),
                MenuItem::action(tr("检查更新…", "Check for Updates…"), CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action(tr("设置…", "Settings…"), OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu(tr("服务", "Services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(tr("隐藏 Quiet.md", "Hide Quiet.md"), HideApp),
                MenuItem::action(tr("隐藏其他", "Hide Others"), HideOthers),
                MenuItem::action(tr("全部显示", "Show All"), ShowAll),
                MenuItem::separator(),
                MenuItem::action(tr("退出 Quiet.md", "Quit Quiet.md"), Quit),
            ],
            disabled: false,
        },
        Menu {
            name: tr("文件", "File").into(),
            items: vec![
                MenuItem::action(tr("新建文档", "New Document"), NewDoc),
                MenuItem::action(tr("新建笔记窗口", "New Note Window"), crate::quick_note::QuickNew),
                MenuItem::action(tr("打开 Markdown 文件…", "Open Markdown File…"), OpenFile),
                MenuItem::action(tr("打开文件夹…", "Open Folder…"), OpenFolder),
                // 打开 uses things where they are; 导入 copies them in: two groups.
                MenuItem::separator(),
                MenuItem::action(tr("导入为文档…", "Import as Document…"), ImportDoc),
                MenuItem::action(tr("导入文件夹…", "Import Folder…"), ImportFolder),
                MenuItem::action(tr("从其他应用导入…", "Import from Another App…"), ImportFromApps),
                MenuItem::separator(),
                MenuItem::action(tr("关闭窗口", "Close Window"), CloseWindow),
            ],
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
        // 显示 (View). Its name per language, not through `tr`: the key 显示 is the verb
        // "show" in the tables (Mostrar, Afficher…); these are the system's View menus.
        Menu {
            name: {
                use crate::settings::Lang as L;
                match crate::settings::lang() { L::Zh => "显示", L::ZhTw => "顯示", L::En => "View", L::Ja => "表示", L::Ko => "보기", L::Fr => "Présentation", L::Es => "Visualización", L::Ar => "عرض", L::Tr => "Görüntü" }.into()
            },
            items: vec![MenuItem::action(tr("切换深色 / 浅色", "Toggle Dark Mode"), ToggleDarkMode)],
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
        KeyBinding::new("cmd-z", crate::editor::keymap::Undo, Some("Workspace")),
        KeyBinding::new("cmd-shift-z", crate::editor::keymap::Redo, Some("Workspace")),
        KeyBinding::new("cmd-y", crate::editor::keymap::Redo, Some("Workspace")),
        KeyBinding::new("cmd-n", NewDoc, None),
        KeyBinding::new("cmd-shift-n", crate::quick_note::QuickNew, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        // For trying the first-launch pages; not listed in Settings › Shortcuts.
        KeyBinding::new("cmd-alt-shift-o", ReplayOnboarding, None),
        KeyBinding::new("cmd-shift-i", ImportDoc, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", OpenFolder, None),
        KeyBinding::new("cmd-alt-home", GoHome, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-]", ToggleSidebar, None),
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
        // ⌥⌘D is the system's Dock hiding; ⌃⌘D is free in the app (Cocoa's 查询 doesn't reach GPUI)
        KeyBinding::new("ctrl-cmd-d", ToggleDarkMode, None),
    ]);
    // Windows: F11 for full screen, and Ctrl+H for 查找和替换 as in Office
    // and Notepad (written cmd-h: `shortcuts::pc_binding` maps cmd to
    // ctrl and drops a literal ctrl-h, Cocoa's backspace).
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("f11", ToggleFullScreen, None), KeyBinding::new("cmd-h", ReplaceInDoc, None)]);
    // Ctrl+Shift+D there (written cmd-shift-d, mapped by `pc_binding`): Ctrl+Alt reads as AltGr on Windows (#290).
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("cmd-shift-d", ToggleDarkMode, None)]);
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
    // 切换深色 / 浅色: the other of the two, kept as the 外观 preference; under 跟随系统
    // the look is the system's, so this is the opposite of the system's. After the
    // key's dispatch, which borrows the window.
    cx.on_action(|_: &ToggleDarkMode, cx| cx.defer(|cx| {
        let Some(h) = cx.windows().into_iter().find_map(|w| w.downcast::<Workspace>()) else { return };
        let to = if crate::theme::dark() { "light" } else { "dark" };
        let _ = h.update(cx, |ws, _, cx| ws.set_pref(|p| p.appearance = to.into(), cx));
    }));
    crate::modes::bind_keys(cx);
}

/// 退出: the open document is saved first; when that fails its dialog asks
/// (重试 / 放弃修改) and quits afterwards. Call it outside any window update
/// (`cx.defer`): a window on the update stack cannot be asked to save.
pub fn quit(cx: &mut App) {
    let ready = cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Workspace>())
        .map(|w| w.update(cx, |ws, _, cx| ws.save_before(cx, |_, _, cx| cx.defer(quit))).unwrap_or(false))
        .fold(true, |ready, saved| ready & saved);
    if ready {
        cx.quit();
    }
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
    /// The "…" button it hangs from (`design::hang`), by its bounds' slot; None: a right-click, at the pointer.
    pub from: Option<String>,
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
#[derive(Clone, Debug)]
pub enum Confirm {
    /// 删除 (to the 回收站); `batch` for the 批量操作 selection.
    Delete { ids: Vec<String>, batch: bool },
    /// 回收站 → 恢复.
    Restore(String),
    /// 回收站 → 彻底删除.
    Purge(String),
    /// 回收站 → 清空回收站: every trashed page, permanently.
    EmptyTrash,
    /// 设置 › 文档库 → 清理未使用资源: the sweep's file names and sizes (#237).
    RemoveUnused(Vec<(String, u64)>),
    /// 隐藏引用列表 (with 不再提示) / 显示引用列表.
    Refs { show: bool, no_ask: bool },
    /// The open document could not be written before it was to be replaced
    /// (another document, 新建, 退出): 重试 / 放弃修改, then carry on.
    SaveFailed,
    /// The library should move to real folders, and the user is asked first
    /// (it is synced, or used from another computer): 改用文件夹 / 稍后.
    Migrate(crate::store::ConsentReason),
    /// 从侧栏移除 a top-level folder (key, name): the folder and its files stay.
    RemoveFolder(String, String),
    /// Dropped on the sidebar from outside: import it into the library there?
    Import { what: DropImport, target: crate::sidebar_drop::SidebarDropAt },
    /// 打开文件夹… on a code project or a very big folder: add it? The first one is asked about.
    AddFolder(Vec<(std::path::PathBuf, fdl_core::shown::Survey)>),
}

/// What a sidebar drop would import: files and folders from the Finder, or a 本机文件.
#[derive(Clone, Debug)]
pub enum DropImport { Files(Vec<std::path::PathBuf>), Local(String) }

/// What to do once the open document is safely on disk (or its changes given up).
pub type AfterSave = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

#[derive(Default)]
/// One top-level folder's lists as a background writer left them (`SharedStore::merge_entries`).
struct ListedDocs { root: String, entries: Vec<crate::store::Entry>, trash: Vec<crate::store::TrashEntry>, warnings: Vec<String> }
struct DeletedDocs { deleted: Vec<String>, errors: Vec<String>, changes: Vec<crate::store::FileDelta> }

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
    /// The page 设置 was opened from, where 返回 goes back to.
    pub settings_from: View,
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
    native_sidebar: crate::native_header::NativeSidebar,
    pub store: SharedStore,
    /// Changed only through `set` / `edit` / `take`, which move its `generation` (entries.rs).
    pub entries: crate::entries::Entries,
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
    /// The sidebar was open at the last render: opening it in a narrow window
    /// widens the window, narrowing the window below it hides it again.
    sidebar_was_open: bool,
    /// The window width before that widening, until the window takes the new size.
    widen_from: Option<f32>,
    /// The keyboard's height last frame (iOS): when it grows, the caret is scrolled back into view.
    keyboard_inset: f32,
    /// Narrowing the window hid the sidebar (not the user): widening it until
    /// the sidebar fits shows it again.
    sidebar_auto_hidden: bool,
    pub folder_current: Option<String>,
    pub create_menu: Option<(Option<String>, Point<Pixels>)>,
    /// 主页's header "…" menu, where it hangs.
    pub place_menu: Option<Point<Pixels>>,
    /// A breadcrumb's "…" open: the crumbs it folds, where its menu hangs.
    pub crumb_menu: Option<(Vec<crate::folders::Crumb>, Point<Pixels>, Anchor)>,
    pub folder_drag: crate::sidebar_drag::SidebarDrag,
    /// Moves and renames shown before they are on disk (`tree_writes.rs`).
    pub(crate) tree_writes: crate::tree_writes::TreeWrites,
    /// This window, for what comes back from the background and needs it.
    pub(crate) window: AnyWindowHandle,
    /// The search dialog (command-center style) is open.
    pub search_open: bool,
    /// The document being read off the main thread because it was still
    /// only in iCloud (#406); opened when it arrives unless another was.
    pub downloading: Option<String>,
    pub status: SaveStatus,
    pub row_menu: Option<RowMenu>,
    pub rename: Option<Rename>,
    pub(crate) file_history: crate::file_history::FileHistory,
    /// Last-frame bounds of each row's "…" button, to hang its menu from.
    pub more_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    /// Header "…" menu and its open submenu.
    pub doc_menu: bool,
    pub doc_sub: Option<DocSub>,
    /// 窗口保持在最前 (the "…" menu): this window floats above other apps'; not remembered.
    pub on_top: bool,
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
    /// The keyboard while 导出 Markdown or 文档信息 dialog (the confirmation has its own, `confirm_focus`) is open.
    pub dialog_focus: crate::design::DialogFocus,
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
    /// Folded sidebar sections: 置顶, (unused: 我的文档 never folds, #421), 本机文件.
    pub folded: [bool; 3],
    /// Top-level folder folders folded shut in the sidebar, by key.
    pub folded_tops: std::collections::HashSet<String>,
    /// Expanded pages of the 我的文档 tree.
    pub expanded: HashSet<String>,
    /// Where a drag from outside the tree (the Finder, 本机文件) would land in the sidebar.
    pub sidebar_drop: Option<crate::sidebar_drop::SidebarDropAt>,
    pub move_dlg: Option<crate::tree::MoveDialog>,
    /// 导出 Markdown 设置.
    pub export_md: Option<crate::transfer::ExportMd>,
    /// 从其他应用导入… while open.
    pub import_apps: Option<crate::transfer::ImportApps>,
    /// 编辑 / 阅读 dropdown, and the mode button's last-frame bounds.
    pub mode_menu: bool,
    pub mode_btn_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The header "…" button, which the doc menu hangs from.
    pub more_btn_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The 快捷键 drawer, and the help button's menu (open while hovered).
    pub shortcuts: bool,
    pub(crate) focus: FocusHandle,
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
    /// Opened while the first listing runs: the start document opens once it is done.
    start_later: bool,
    /// The loading layer covered the note area when the listing came back: it fades out there.
    load_cover: bool,
    _subs: Vec<Subscription>,
}


/// A macOS notification (only logged when scripted).
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
        let me = cx.entity_id();
        for handle in cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>()) {
            if !handle.is_active(cx).is_some_and(|yes| yes) || handle.entity(cx).is_ok_and(|e| e.entity_id() == me) { continue }
            let (entries, trash) = (entries.to_vec(), trash.map(<[_]>::to_vec));
            let _ = handle.update(cx, |ws, _, cx| {
                ws.entries.set(entries); if let Some(trash) = trash { ws.trash = trash; } ws.sort();
                cx.notify();
            });
        }
    }

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
        crate::settings::check_fonts(cx);
        // 文档字体's download list shows progress and what arrived.
        cx.observe_global::<crate::font_downloads::Tick>(|ws, cx| if ws.font_menu.is_some() { cx.notify() }).detach();
        cx.observe_global::<crate::font_downloads::Changed>(|_, cx| cx.notify()).detach();
        if !cx.has_global::<SharedStore>() { cx.set_global(SharedStore::open()); }
        let mut store = cx.global::<SharedStore>().clone();
        // A big library's first listing goes on in the background (`loading`).
        let listed = crate::loading::begin(&mut store, cx);
        let start_later = listed.is_none();
        let entries = listed.unwrap_or_default();
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
                if window.is_window_active() {
                    if let Some(id) = &this.current { crate::quick_note::used(id, cx); }
                    // Several windows share one top-level folder in front: the one coming forward
                    // puts its own back (its pasted images, its look).
                    if let Some(id) = this.current.clone().or_else(|| this.folder_current.clone()) && this.store.set_active(&id) { cx.defer(crate::settings::top_switched); }
                    // A write of the tree on its way holds the library's lock, and lists it when it lands.
                    if !this.store.file_task_busy() && !this.store.loading() && !this.tree_writes_pending() { this.entries.set(this.store.list()); this.sort(); this.refresh_external(Some(window.window_handle().window_id()), cx); }
                    crate::update::check_if_stale(cx); cx.notify();
                }
                else if !this.store.file_task_busy() { this.cancel_home_drag(cx); this.end_folder_drag(true, cx); this.cancel_shortcut_recording(cx); if !this.tree_writes_pending() { this.flush(cx); } }
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
                this.flush_view_state();
                // A rename or move still on its way is made before the app goes.
                if !this.store.file_task_busy() { this.settle_tree_writes(cx); }
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
            this.update(cx, |w, cx| { w.cancel_shortcut_recording(cx); w.flush_view_state(); w.save_before(cx, |_, window, _| window.remove_window()) }).unwrap_or(true)
        });
        if !quick { crate::settings::watch_themes(cx); }
        // A file written into the library from outside (a model writing a
        // note, say) joins it without going away and back: each top-level folder's
        // shape is compared every couple of seconds, and those that changed are
        // listed again off the main thread (`relist`). So is the open document: a
        // clean editor takes in what another program (the `quiet` CLI, say) wrote to it.
        cx.spawn(async move |this, cx| {
            let bg = cx.background_executor().clone();
            let shape = |dirs: Vec<(String, std::path::PathBuf)>| bg.spawn(async move { dirs.into_iter().map(|(k, d)| (k, crate::store::Store::shape_at(&d))).collect::<Vec<_>>() });
            let dirs = |cx: &mut AsyncApp| this.update(cx, |w, _| w.store.top_keys_dirs()).unwrap_or_default();
            let mut last = shape(dirs(cx)).await;
            let mut last_file = None;
            loop {
                bg.timer(Duration::from_secs(2)).await;
                let now = shape(dirs(cx)).await;
                let Ok(path) = this.update(cx, |w, _| w.current.as_ref()
                    .filter(|id| !w.store.file_task_busy() && !w.tree_writes_pending() && w.store.kind(id) == crate::store::DocKind::Doc)
                    .and_then(|id| w.store.file_path(id))) else { break };
                let file = bg.spawn(async move {
                    let path = path?;
                    let meta = fdl_core::vfs::metadata(&path).ok()?;
                    Some((path, meta.modified().ok()?, meta.len()))
                }).await;
                if file != last_file {
                    let _ = this.update(cx, |w, cx| w.refresh_external(None, cx));
                    last_file = file;
                }
                if now == last { continue }
                // Only what changed; a top-level folder just added is listed as it is added.
                let changed: Vec<String> = now.iter().filter(|(k, s)| last.iter().any(|(l, t)| l == k && t != s)).map(|(k, _)| k.clone()).collect();
                let reloaded = this.update(cx, |w, cx| {
                    // Not mid-rename: the row under the field would be rebuilt.
                    if w.rename.is_some() || w.move_dlg.is_some() || w.store.file_task_busy() || w.store.loading() || w.tree_writes_pending() { return false }
                    w.relist(changed, cx)
                });
                match reloaded { Ok(true) => last = now, Ok(false) => {}, Err(_) => break }
            }
        })
        .detach();
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
        let settings = store.settings();
        let sidebar_width = settings.sidebar_width.unwrap_or(theme::SIDEBAR_WIDTH);
        let history_limit = store.history_limit();
        let mut ws = Workspace {
            quick_note: quick,
            shortcut_recording: None,
            shortcut_strokes: vec![],
            shortcut_error: None,
            settings_page: Default::default(),
            settings_from: View::Home,
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
            store,
            entries: entries.into(),
            current: None,
            editor: None,
            search,
            search_sel: None,
            sidebar_open: !quick,
            sidebar_width,
            sidebar_was_open: false,
            widen_from: None,
            keyboard_inset: 0.,
            sidebar_auto_hidden: false,
            sidebar_drag: false,
            folder_current: None,
            create_menu: None,
            place_menu: None,
            crumb_menu: None,
            folder_drag: Default::default(),
            tree_writes: Default::default(),
            window: window.window_handle(),
            search_open: false,
            downloading: None,
            status: SaveStatus::Idle,
            row_menu: None,
            rename: None,
            file_history: Default::default(),
            more_bounds: Default::default(),
            doc_menu: false,
            on_top: false,
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
            dialog_focus: crate::design::DialogFocus::new(cx),
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
            folded_tops: Default::default(),
            expanded: HashSet::new(),
            sidebar_drop: None,
            move_dlg: None,
            export_md: None,
            import_apps: None,
            focus: cx.focus_handle(),
            save_task: None,
            view_state_task: None,
            dirty_since: None,
            unsaved: false,
            after_save: None,
            editor_sub: None,
            start_later: start_later && document.is_none() && !quick,
            load_cover: false,
            _subs: subs,
        };
        ws.sort();
        ws.load_home();
        // An empty library opens on 主页's empty state; only a quick note
        // window starts a document of its own. The main window returns to the
        // document read last (#234), or the home page when that is set; the
        // site's live demo always starts on 主页, so visitors see the whole
        // notebook first.
        match document {
            Some(id) => ws.open(&id, window, cx),
            None if quick => ws.new_quick_note(window, cx),
            // Still listing: 主页 (under the loading layer) until it is done; the
            // window's keys work meanwhile (⌘N and search say it is still loading).
            None if ws.start_later => { ws.view = View::Home; window.focus(&ws.focus, cx); }
            None => ws.open_start(window, cx),
        }
        ws.show_warnings(cx);
        ws
    }

    /// Where the main window starts: the document read last (#234), or 主页
    /// when that is set; the site's live demo always starts on 主页, so
    /// visitors see the whole notebook first.
    fn open_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::settings::prefs().start_home || cfg!(target_family = "wasm") { self.view = View::Home; return }
        // Nothing read yet (a fresh library, whose guides all start at
        // `opened == 0`): the first document, as before — `max_by_key`
        // keeps the last of equal maxima, which would open the last
        // guide instead.
        let id = self.entries.iter().filter(|e| e.meta.opened > 0).max_by_key(|e| e.meta.opened).or_else(|| self.entries.first()).map(|e| e.id.clone());
        match id {
            Some(id) => self.open(&id, window, cx),
            None => self.view = View::Home,
        }
    }

    /// Problems opening the libraries: kept in the top bar (not just a toast), the user should notice.
    fn show_warnings(&mut self, cx: &mut Context<Self>) {
        let warnings = self.store.take_warnings();
        if !warnings.is_empty() {
            let msg = warnings.iter().map(|s| crate::settings::diagnostic(s)).collect::<Vec<_>>().join(crate::settings::tr("；", "; "));
            self.show_toast_kind(ToastKind::Error, &msg, cx);
            self.status = SaveStatus::Failed(msg);
        }
    }

    /// The first listing came back (`loading`): the rows, and the document
    /// the window starts on unless one was opened meanwhile. The loading
    /// layer fades out over whatever the note area shows then.
    pub(crate) fn loaded(&mut self, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) {
        self.entries.set(entries);
        self.sort();
        self.load_home();
        self.load_cover = matches!(self.view, View::Home | View::Folder);
        if std::mem::take(&mut self.start_later) && self.current.is_none() && self.view == View::Home { self.open_start(window, cx); }
        self.show_warnings(cx);
        cx.notify();
    }

    /// Top-level folders `keys` listed again off the main thread (the 2 s look): each
    /// library takes over its writer when it wrote nothing meanwhile, else is listed
    /// here. False when another listing is still running (try again later).
    pub(crate) fn relist(&mut self, keys: Vec<String>, cx: &mut Context<Self>) -> bool {
        if keys.is_empty() { return true }
        let Some(writers) = self.store.begin_relist(&keys) else { return false };
        let task = cx.background_executor().spawn(async move {
            writers.into_iter().map(|(k, n, mut w)| { let e = w.list(); (k, n, w, e) }).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let done = task.await;
            let _ = this.update(cx, |w, cx| {
                let entries = w.store.end_relist(done);
                // Not mid-rename: the row under the field would be rebuilt; the next look brings it.
                if w.rename.is_some() || w.move_dlg.is_some() { return }
                w.entries.set(entries);
                w.sort();
                cx.notify();
            });
        }).detach();
        true
    }

    /// While the first listing runs, what needs the whole library (search,
    /// 新建, imports) says so and does nothing.
    pub(crate) fn still_loading(&mut self, cx: &mut Context<Self>) -> bool {
        if !crate::loading::active(cx) { return false }
        self.show_toast_kind(ToastKind::Info, crate::settings::tr("文档库还在载入，稍后再试", "The library is still loading; try again in a moment"), cx);
        true
    }

    /// Most recently modified first.
    pub(crate) fn sort(&mut self) {
        self.reapply_pending_tree_writes();
        self.entries.edit().sort_by(|a, b| b.meta.modified.cmp(&a.meta.modified).then_with(|| a.id.cmp(&b.id)));
        self.sync_note_rows();
    }

    pub fn query(&self, cx: &App) -> String {
        self.search.read(cx).text.trim().to_lowercase()
    }

    pub fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.try_open(id, window, cx);
    }

    /// Shows 正在从 iCloud 下载, reads `path` in the background (the system
    /// downloads it to answer) and opens `id` once it is here, unless another
    /// document was opened meanwhile. No retry when the read fails.
    fn download_then_open(&mut self, id: &str, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.downloading = Some(id.to_string());
        self.show_toast(crate::settings::tr("正在从 iCloud 下载…", "Downloading from iCloud…"), cx);
        let target = id.to_string();
        let read = cx.background_spawn(async move { std::fs::read(&path).map(|_| ()) });
        cx.spawn_in(window, async move |this, cx| {
            let r = read.await;
            let _ = this.update_in(cx, |w, window, cx| {
                if w.downloading.as_deref() != Some(target.as_str()) { return }
                w.downloading = None;
                match r {
                    // Listed by its name until now: its title and words come with it.
                    Ok(()) if w.store.cloud_only(&target).is_none() => { w.entries.set(w.store.list()); w.sort(); w.open(&target, window, cx); }
                    Ok(()) => w.show_toast_kind(ToastKind::Error, crate::settings::tr("这篇文档还没有从 iCloud 下载完成", "This document hasn't finished downloading from iCloud"), cx),
                    Err(e) => w.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法从 iCloud 下载这篇文档：{}", "Could not download this document from iCloud: {}", e), cx),
                }
                cx.notify();
            });
        }).detach();
    }

    /// Show `id`; false when it could not be opened (or the open document
    /// could not be saved first, see [`Self::save_before`]).
    pub fn try_open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // A snapshot is on screen: back to the live document before anything
        // else touches the editor.
        if self.history.is_some() { self.close_history(cx); }
        // 阅读位置: remember where the document being left was, before the
        // editor is swapped out.
        self.remember_scroll(cx);
        // A folder with a 正文 opens as that document (#357).
        if let Some(note) = self.entries.iter().find(|e| e.id == id).and_then(|e| e.note.clone()) { return self.try_open(&note, window, cx); }
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) { return self.open_folder(id, window, cx); }
        if self.view != View::Doc {
            self.view = View::Doc;
            cx.notify();
        }
        if self.current.as_deref() == Some(id) {
            return true;
        }
        let target = id.to_string();
        // A rename or move on its way: a page it moves opens once it has landed (its file is
        // not where the library last saw it), and so does any page while this one has to be saved first.
        if (self.unsaved || self.awaits_tree_write(id)) && { let later = target.clone(); self.after_tree_writes(move |w, window, cx| w.open(&later, window, cx)) } { return false; }
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
        // Still only in iCloud (#406): reading it downloads it, which can take
        // as long as the network does, so it is read off the main thread.
        if let Some(path) = self.store.cloud_only(id) {
            if self.downloading.as_deref() != Some(id) { self.download_then_open(id, path, window, cx); }
            return false;
        }
        self.downloading = None;
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
            e.board_file = meta.as_ref().is_some_and(|m| m.kind == crate::store::DocKind::Board);
            e.table_file = meta.as_ref().is_some_and(|m| m.kind == crate::store::DocKind::Table);
            if let Some(m) = &meta {
                e.created = m.created;
                e.info = m.info;
                if !e.board_file && !e.table_file { e.set_read_only(m.reading, cx); }
                // a build without 数据表 shows a table file's page as a placeholder, not to be edited
                if e.table_file && !crate::DATATABLE { e.set_read_only(true, cx); }
            }
            e
        });
        // Reuse the live state, including unsaved edits and undo history.
        if let Some(peer) = self.document_peers(id, window, cx).first() {
            let core = peer.read(cx).core.clone();
            editor.update(cx, |e, cx| e.receive_document(&core, cx));
        }
        // 阅读位置: back to where this document was left, on its first frame.
        let top = self.store.scroll_top(id);
        editor.update(cx, |e, _| e.restore_top = top);
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
                if !this.flush(cx) { this.confirm = Some(Confirm::SaveFailed); cx.notify(); }
            }
            EditorEvent::Search(q) => {
                this.open_search(window, cx);
                this.search.update(cx, |s, cx| s.set_text(q, cx));
                this.search_sel = this.default_search_sel(cx);
            }
            EditorEvent::OpenRelative(url) => this.open_relative(url, window, cx),
            EditorEvent::OpenDoc(id) => this.open(id, window, cx),
            EditorEvent::OpenDocAt(id, ordinal) => this.open_at(id, *ordinal, window, cx),
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
                    if let Some(e) = this.entries.edit().iter_mut().find(|e| e.id == id) {
                        e.meta.catalogue_closed = !pinned;
                    }
                    let _ = this.store.set_catalogue(&id, *pinned);
                }
            }
        }));
        // Opening a document puts the caret in it, unless the user is typing a search.
        // Not on iOS, where a focused editor brings up the keyboard: a tap in the text does.
        if !self.search_open && !cfg!(target_os = "ios") {
            { let f = editor.read(cx).focus.clone(); window.focus(&f, cx); }
        }
        // A 画板 opens straight into its board, over the whole window.
        editor.update(cx, |e, cx| { e.open_board_file(window, cx); e.open_table_file(window, cx); });
        if let Some(m) = self.store.touch(id)
            && let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) {
                e.meta.opened = m.opened;
            }
        self.schedule_view_state(cx);
        if let Some(previous) = &self.current
            && self.document_peers(previous, window, cx).is_empty() { self.store.forget_loaded(previous); }
        self.editor = Some(editor);
        self.current = Some(id.to_string());
        // Pasted images, 数据表 and 画板 belong to this document's top-level folder, and it has its own look.
        if self.store.set_active(id) { cx.defer(crate::settings::top_switched); }
        // Web images pasted earlier whose download landed after it closed (#220).
        if let Some(e) = self.editor.clone() { e.update(cx, |e, cx| e.adopt_remote(cx)); }
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

    /// ⌘N / the 文件 menu: create the 快捷新建 type (a fixed one, or the last
    /// one made from the 新建 menu) in the current place.
    pub fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self.creation_parent();
        self.create_doc(crate::settings::quick_new_markdown(), parent, window, cx);
    }

    /// A 快速笔记 window's own note: at the top of the top-level folder Settings › 文档库
    /// picks (2.8), else the first open top-level folder.
    fn new_quick_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self.store.quick_note_folder().map(|k| crate::shared_store::top_row(&k));
        self.create_doc(crate::settings::quick_new_markdown(), parent, window, cx);
    }

    /// Ask before 删除: a confirmation, then the 回收站.
    pub fn ask_delete(&mut self, id: &str, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        // A document with children goes with them, folder and all.
        self.confirm = Some(Confirm::Delete { ids: vec![self.row_of(id)], batch: false });
        cx.notify();
    }

    /// Move documents to the trash; if the open one goes, open the next.
    pub fn delete_docs(&mut self, ids: &[String], window: &mut Window, cx: &mut Context<Self>) {
        if self.store.file_task_busy() || self.file_history.busy { return; }
        let later = ids.to_vec();
        if self.after_tree_writes(move |w, window, cx| w.delete_docs(&later, window, cx)) { return; }
        // A top-level folder's row is never deleted this way: it leaves the sidebar (从侧栏移除).
        let ids: Vec<String> = ids.iter().filter(|id| crate::shared_store::top_of_row(id).is_none()).cloned().collect();
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
        // A selection may span top-level folders: one writer, and one write, per top-level folder.
        let mut writers: Vec<(String, crate::store::Store, Vec<(String, Option<String>)>)> = vec![];
        for g in groups {
            let Some(key) = self.store.top_of(&g.0) else { continue };
            match writers.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, v)) => v.push(g),
                None => if let Some(w) = self.store.top_writer(&key) { writers.push((key, w, vec![g])); },
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
        self.entries.edit().retain(|e| !all.contains(&e.id));
        self.batch = None;
        self.row_menu = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let _task_guard = task_guard;
            let (done, lists) = task.await;
            let _ = this.update_in(cx, |w, window, cx| {
                let mut listed = ListedDocs { root: String::new(), entries: w.entries.to_vec(), trash: w.trash.clone(), warnings: vec![] };
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
                w.entries.set(listed.entries); w.trash = listed.trash;
                if w.folder_current.as_ref().is_some_and(|id| done.deleted.contains(id)) {
                    w.folder_current = None;
                    if w.view == View::Folder { w.view = View::Home; }
                }
                w.row_menu = None;
                if w.current.is_none() && w.view != View::Folder {
                    let view = w.view;
                    match w.entries.first().map(|e| e.id.clone()) {
                        Some(next) => { w.open(&next, window, cx); w.view = view; }
                        None => w.view = if view == View::Doc { View::Home } else { view },
                    }
                }
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
        let mut on = false;
        if let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) {
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
        let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) else { return };
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
        self.cancel_shortcut_recording(cx);
        self.close_history(cx);
        // 阅读位置: leaving the document with ⌘H or 回收站 also counts.
        if view != View::Doc { self.remember_scroll(cx) }
        // 设置's 返回 goes back here. 回收站 sits in 设置's sidebar: going there and back keeps it.
        if matches!(view, View::Settings | View::Trash) && !matches!(self.view, View::Settings | View::Trash) { self.settings_from = self.view; }
        if view == View::Trash {
            self.trash = self.store.list_trash();
        }
        self.view = view;
        self.row_menu = None;
        self.trash_menu = None;
        self.theme_menu = None;
        self.lang_menu = None;
        self.pref_menu = None;
        self.close_font_menu(cx);
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
        // A top-level folder's row renames its folder on disk (2.11), where that is allowed.
        if crate::shared_store::top_of_row(id).is_some_and(|k| !self.store.can_rename_folder_top(k)) { return; }
        let Some(title) = self.entries.iter().find(|e| e.id == id).map(|e| e.title.trim().to_string()) else { return };
        let field = cx.new(|cx| {
            let mut f = TextField::new(crate::model::UNTITLED, cx);
            f.set_text(&title, cx);
            f.select_all_text(cx);
            f
        });
        let focus = field.focus_handle(cx);
        window.focus(&focus, cx);
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
        if let Some(key) = crate::shared_store::top_of_row(&r.id) {
            if commit && old.is_some_and(|o| o != title) { self.rename_folder_top(key, &title, window, cx); }
            cx.notify();
            return;
        }
        // A document with children is renamed through its 正文, which renames the folder too.
        let row = r.id.clone();
        let r = Rename { id: self.note_or(&r.id), ..r };
        if commit && old.is_some_and(|o| o != title) {
            if let Some(peer) = self.document_peers(&r.id, window, cx).first() {
                peer.update(cx, |e, cx| e.set_title(&title, cx));
            } else {
                // The open document's words are on disk before its file is written with the new title.
                if self.current.as_deref() == Some(r.id.as_str()) && !self.flush(cx) { return; }
                // Shown now, written off this thread (`tree_writes.rs`); a failure comes back to this field.
                self.rename_ahead(&row, &r.id, &title, cx);
            }
        }
        if refocus
            && let Some(ed) = &self.editor {
                { let f = ed.read(cx).focus.clone(); window.focus(&f, cx); }
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

    /// 阅读位置: hand the open document's scroll offset to the library, so
    /// coming back to it lands where it was left.
    fn remember_scroll(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(editor)) = (self.current.clone(), self.editor.clone()) else { return };
        let top = editor.read(cx).scroll_top();
        self.store.remember_scroll(&id, top);
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
        for peer in peers { peer.update(cx, |e, cx| e.receive_document(&core, cx)); }
        for handle in cx.windows().into_iter().filter(|w| w.window_id() != window.window_handle().window_id()).filter_map(|w| w.downcast::<Workspace>()) {
            let _ = handle.update(cx, |ws, _, cx| {
                if let Some(entry) = ws.entries.edit().iter_mut().find(|e| e.id == id) {
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
            if let Some(e) = self.entries.edit().iter_mut().find(|e| &e.id == id) {
                e.title = title;
            }
        }
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            Self::tree_writes_landed(&this, cx).await;
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

    /// The current window can already be in a GPUI input callback. update
    /// skips windows on the stack, whereas reading their top-level folder would panic.
    pub(crate) fn other_workspaces(cx: &mut Context<Self>) -> Vec<Entity<Self>> {
        let me = cx.entity_id();
        cx.windows().into_iter().filter_map(|h| h.update(cx, |root, _, _| root.downcast::<Self>().ok()).ok().flatten())
            .filter(|peer| peer.entity_id() != me).collect()
    }

    /// `on_stack`: the window being updated, if any (it can't be read now; it is this one).
    fn refresh_external(&mut self, on_stack: Option<gpui::WindowId>, cx: &mut Context<Self>) {
        if self.preview.is_some() || self.store.file_task_busy() || self.tree_writes_pending() || matches!(self.confirm, Some(Confirm::SaveFailed)) { return }
        let Some(id) = self.current.clone() else { return };
        if !self.unsaved { self.follow_moved_links(cx); }
        if self.store.kind(&id) != crate::store::DocKind::Doc { return }
        if self.unsaved {
            self.flush(cx);
            return
        }
        // A newly opened sibling can be clean while its source view still has a pending edit.
        if cx.windows().into_iter().filter(|h| Some(h.window_id()) != on_stack).filter_map(|h| h.downcast::<Workspace>()?.entity(cx).ok())
            .filter(|p| p.entity_id() != cx.entity_id())
            .any(|p| { let w = p.read(cx); w.current.as_ref() == Some(&id) && w.unsaved }) { return }
        if let Ok(Some(doc)) = self.store.with(Some(&id), |s| s.refresh_external(&id)) {
            self.adopt_external(&id, doc, cx);
        }
    }

    /// A move rewrote the open document's links to files on disk
    /// (`Store::follow_moved_links`): every window showing it follows, so no
    /// save writes the old paths back. Each link as it was → as it is now.
    pub(crate) fn follow_moved_links(&mut self, cx: &mut Context<Self>) -> Option<HashMap<String, String>> {
        let id = self.current.clone().filter(|_| self.preview.is_none())?;
        let map = self.store.with(Some(&id), |s| Ok(s.follow_moved_links(&id))).ok().flatten()?;
        let relink = |w: &mut Self, cx: &mut Context<Self>| if let Some(ed) = &w.editor {
            ed.update(cx, |e, cx| if e.core.relink_files(&|l| map.get(l).cloned()) { cx.notify(); });
        };
        relink(self, cx);
        for peer in Self::other_workspaces(cx) {
            if peer.read(cx).current.as_deref() == Some(id.as_str()) { peer.update(cx, |w, cx| relink(w, cx)); }
        }
        Some(map)
    }

    /// All live views share the store's save baseline, so install the accepted
    /// revision in each of them before a sibling's pending autosave can run.
    fn adopt_external(&mut self, id: &str, doc: crate::model::Document, cx: &mut Context<Self>) {
        let core = crate::editor::core::Core::new(doc);
        let install = |w: &mut Self, cx: &mut Context<Self>| {
            if let Some(p) = &mut w.preview { p.core = core.clone(); }
            else if let Some(ed) = &w.editor { ed.update(cx, |e, cx| e.receive_markdown(&core, cx)); }
            w.save_task = None;
            w.unsaved = false;
            w.dirty_since = None;
            w.status = SaveStatus::Saved;
            if let Some(entry) = w.entries.edit().iter_mut().find(|e| e.id == id) {
                entry.title = core.doc.title.text.clone();
                entry.body = core.doc.plain_text().into();
                entry.reminders = core.doc.reminders();
                entry.links = core.doc.doc_links();
            }
            cx.notify();
        };
        install(self, cx);
        for peer in Self::other_workspaces(cx) {
            if peer.read(cx).current.as_deref() == Some(id) {
                peer.update(cx, install);
            }
        }
    }

    /// Write the open document to disk if it has unsaved changes. False when
    /// that failed: the changes stay pending (a later save retries them) and
    /// the top bar shows the error.
    pub fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        self.save_task = None;
        // 数据表 write their own files; a cell still being edited goes in first (leaving the page drops its view)
        if let Some(ed) = self.editor.clone() { ed.update(cx, |e, cx| e.commit_tables(cx)); }
        crate::table::flush_all(cx);
        // 版本历史 preview: the editor holds an older snapshot, so writing it
        // would overwrite the live document with history.
        if self.preview.is_some() { return true }
        if !self.unsaved {
            return true;
        }
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else {
            self.unsaved = false;
            return true;
        };
        // A rename or move on its way lands first: this save then goes where the file is.
        self.settle_tree_writes(cx);
        self.follow_moved_links(cx);
        let doc = &ed.read(cx).core.doc;
        let (title, icon, body, reminders, links) = (doc.title.text.clone(), doc.icon.clone(), doc.plain_text(), doc.reminders(), doc.doc_links());
        match self.store.with(Some(&id), |s| s.save_editor_naming_later(&id, doc)) {
            Ok((meta, name_due)) => {
                // The file's name follows the new title off this thread (links to it are rewritten, a 正文's folder moves).
                if name_due { self.queue_tree_write(crate::tree_writes::TreeWrite::Rename(crate::tree_writes::PendingRename { id: id.clone(), title: title.trim().to_string(), gesture: None }), cx); }
                if let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) {
                    e.title = title;
                    e.icon = icon;
                    e.body = body.into();
                    e.reminders = reminders;
                    e.links = links;
                    e.meta = self.store.as_row(&id, meta.clone());
                }
                // A mirrored edit was saved by this window. A sibling must not
                // later write it a second time over a newer external edit.
                if meta.markdown {
                    for peer in Self::other_workspaces(cx) {
                        let w = peer.read(cx);
                        if w.unsaved && w.current.as_ref() == Some(&id) && w.preview.is_none()
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
    /// 改用文件夹: move the library to real folders, then reopen the main
    /// window on it at the same place (documents get new ids in the new layout).
    pub(crate) fn migrate_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.save_before(cx, |w, window, cx| w.migrate_library(window, cx)) { return }
        let root = crate::store::Store::active_dir();
        // Reopened under the same folder key: what this computer keeps about it stays.
        let key = self.store.with(None, |s| Ok(s.key().to_string())).unwrap_or_else(|_| crate::store::library_key(&root));
        let moved = crate::store::Store::migrate(&root).and_then(|r| Ok((r, crate::store::Store::open_at_as(root.clone(), &key)?)));
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
        } else {
            let Some(id) = self.current.clone() else { return };
            match self.store.load_for_edit(&id, false) {
                Ok(doc) => {
                    if self.store.is_markdown(&id) { self.adopt_external(&id, doc, cx); }
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
                    self.entries.set(self.store.list());
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
                self.entries.set(self.store.list());
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
            && let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id)
        {
            e.meta.refs_hidden = hidden;
        }
        self.sync_links(cx);
        cx.notify();
    }

    /// `pane_w`: the width the header spans, to fold the breadcrumbs when narrow.
    fn top_bar(&self, pane_w: f32, cx: &mut Context<Self>) -> impl IntoElement {
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
        // Independent icon controls, with equal top and right insets.
        let seg = crate::ui::header_icon;
        let divider = || crate::design::Separator::new().vertical().h(px(18.));
        // Breadcrumbs: the parent pages, each linking to itself, with the
        // middle ones folded into "…" when the chain is long.
        let chain: Vec<(String, String, Option<String>)> = self
            .current
            .as_ref()
            .map(|id| self.ancestors(id).iter().filter(|e| crate::shared_store::top_of_row(&e.id).is_none()).map(|e| (e.id.clone(), display_title(&e.title), e.icon.clone())).collect())
            .unwrap_or_default();
        let mut crumbs: Vec<AnyElement> = vec![];
        let sep = || div().mx(px(3.)).flex_none().child(icon("chevron-right", 12., Theme::text_caption())).into_any_element();
        // Every path starts at the library, as a folder page's does; a file
        // under 本机文件 starts there, which has no page of its own.
        let local = self.current.as_ref().and_then(|id| self.entries.iter().find(|e| &e.id == id)).is_some_and(|e| e.meta.local());
        let start = if local { crate::settings::tr("本机文件", "Files on this computer").to_string() } else {
            let root = self.current.as_ref().and_then(|id| self.entries.iter().find(|e| &e.id == id)).map(|e| e.top.clone()).unwrap_or_default();
            self.folder_name(&root)
        };
        // 主页 is the one root: every path starts there (a folder page's too), then the top-level folder.
        let top_key = self.current.as_ref().and_then(|id| self.entries.iter().find(|e| &e.id == id)).map(|e| e.top.clone()).unwrap_or_default();
        let top_target = if local { None } else { self.top_crumb(&top_key) };
        let home_label = crate::settings::text("主页").to_string();
        if self.current.is_some() {
            crumbs.push(div().id("crumb-home").no_drag().flex_none().px(px(4.)).rounded(px(4.))
                .whitespace_nowrap().text_color(Theme::text_caption()).cursor_pointer().hover(|s| s.text_color(Theme::text()))
                .on_click(cx.listener(|w, _, _, cx| w.show_view(View::Home, cx)))
                .child(ui_text(home_label.clone()))
                .into_any_element());
            crumbs.push(sep());
            let target = top_target.clone();
            crumbs.push(div().id("crumb-library").no_drag().flex_none().px(px(4.)).rounded(px(4.))
                .whitespace_nowrap().text_color(Theme::text_caption())
                .when(!local, |d| d.cursor_pointer().hover(|s| s.text_color(Theme::text())).on_click(cx.listener(move |w, _, window, cx| match &target {
                    Some(t) => w.open(t, window, cx),
                    None => w.show_view(View::Home, cx),
                })))
                .child(ui_text(start.to_string()))
                .into_any_element());
            crumbs.push(sep());
        }
        for (i, (id, name, em)) in chain.iter().enumerate() {
            if chain.len() > 2 && i > 0 && i < chain.len() - 1 {
                if i == 1 {
                    let hidden: Vec<crate::folders::Crumb> = chain[1..chain.len() - 1].iter().map(|c| (c.1.clone(), Some(c.0.clone()))).collect();
                    crumbs.push(self.crumb_more("crumb-more", hidden, cx).into_any_element());
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
                    .hover(|s| s.text_color(Theme::text()))
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
                    .text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::text()))
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
                    .text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::text()))
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
            let path: Vec<crate::folders::Crumb> = [(home_label.clone(), None), (start.clone(), top_target.clone())].into_iter()
                .chain(chain.iter().map(|c| (c.1.clone(), Some(c.0.clone())))).collect();
            crumbs = vec![self.crumb_more("crumb-more", path, cx).into_any_element(), sep()];
        }
        let record_more = self.more_btn_bounds.clone();
        let doc_open = self.doc_menu;
        div()
            .id("top-bar").relative()
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
                    crate::design::Button::ghost("open-sidebar")
                        .no_drag()
                        .ml(px(4.))
                        .icon("panel-left-open")
                        .tip(crate::settings::tr("展开侧栏", "Show sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                        .on_click(cx.listener(|w, _, _, cx| {
                            w.sidebar_open = true;
                            cx.notify();
                        })),
                )
                .child(
                    crate::design::Button::ghost("header-home").no_drag().ml(px(4.)).icon("home")
                        .tip(crate::settings::text("返回主页"), None)
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
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .text_size(px(crate::ui::CRUMB_SIZE))
                            .line_height(px(20.))
                            .children(crumbs)
                            .child(
                                div()
                                    .id("top-title")
                                    .min_w_0()
                                    .text_size(px(crate::ui::CRUMB_SIZE))
                                    .line_height(px(20.))
                                    // the current page reads like the crumbs before it (the user's call, 2026-11-25;
                                    // shadcn's BreadcrumbPage is `foreground`): one colour and weight along the path
                                    .text_color(Theme::text_caption())
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
                    )),
            )
            .child(div().flex_1().min_w(px(16.)))
            .child(
                div().gap(px(4.))
                    .h(px(HEADER_ACTIONS_H)).flex_none()
                    .flex()
                    .items_center()
                    .when(self.current.is_some(), |d| {
                        d.child(
                            // No tooltip while its dropdown is open; the menu hangs
                            // from this button's right edge.
                            seg("doc-more", "ellipsis")
                                .relative()
                                .selected(self.doc_menu)
                                .when(!self.doc_menu, |d| d.tip(crate::settings::tr("更多", "More"), None))
                                .child(canvas(move |b, _, _| record_more.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                                // Toggle on mouse-down from the render-time state: the menu's outside-press
                                // handler has already cleared `doc_menu` by the time a click would fire.
                                .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, _, cx| {
                                    cx.stop_propagation();
                                    w.doc_menu = !doc_open;
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
                    .child(
                        // Straight to a new document: no 新建 menu, as 导入 is rare.
                        seg("doc-new", "plus").tip(crate::settings::tr("新建文档", "New document"), None).on_click(cx.listener(|w, _, window, cx| w.new_doc(window, cx))),
                    ).into_any_element()
            )
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
        crate::design::menu::set_viewport(window);
        self.sync_note_rows();
        // The menu bar is the front window's: a Markdown file's 插入 / 格式 leave out what it can't hold.
        if window.is_window_active() {
            let markdown = self.view == View::Doc && self.editor.as_ref().is_some_and(|e| e.read(cx).markdown);
            cx.defer(move |cx| menus_follow(markdown, cx));
        }
        // Clicks no longer take focus from the editor (see the root's mouse
        // down), so leaving the document hands it back to the window's keys.
        if self.view != View::Doc && self.editor.as_ref().is_some_and(|e| e.read(cx).focus.is_focused(window)) { window.focus(&self.focus, cx); }
        self.sync_confirm_focus(window, cx);
        crate::editor::image::frame_drawn(window, cx);
        // a 数据表 file renamed or moved (the sidebar, 移动到): its view writes where the file now is
        if let (Some(id), Some(ed)) = (&self.current, &self.editor) && ed.read(cx).table_file && let Some(p) = self.store.file_path(id) { ed.update(cx, |e, cx| e.table_file_at(&p, cx)); }
        // Full screen: the sidebar is solid (`Theme::window`) and the native glass stays hidden.
        crate::theme::set_fullscreen(window.is_fullscreen());
        self.native_sidebar.sync(if self.sidebar_open && !crate::theme::fullscreen() { self.sidebar_w(window) } else { 0. }, window);
        // The pane keeps its minimum beside the sidebar: opening the sidebar in
        // a narrow window widens the window rather than squeezing the note, and
        // narrowing the window below that (half-screen snapping) hides the
        // sidebar instead of being refused. The window's own minimum is DOC_MIN_W.
        let vs = window.viewport_size();
        let vw = f32::from(vs.width);
        // iOS: the status bar, the home indicator and the keyboard cover the window's
        // edges (zero elsewhere). The window shrinks above the keyboard; the sidebar
        // and the note's contents keep clear of the bars while the note's page runs
        // under them. A taller keyboard scrolls the caret back above it.
        let insets = window.insets();
        let (inset_top, inset_bottom) = (f32::from(insets.safe_area.top), f32::from(insets.safe_area.bottom));
        let keyboard = f32::from(insets.ime.bottom);
        if keyboard > self.keyboard_inset + 0.5 && let Some(ed) = &self.editor { ed.update(cx, |e, cx| { e.autoscroll = true; cx.notify(); }); }
        self.keyboard_inset = keyboard;
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
        let sidebar = if self.sidebar_open { Some(if matches!(self.view, View::Settings | View::Trash) { self.render_settings_sidebar(window, cx) } else { self.render_sidebar(window, cx) }) } else { None };
        let row_menu = self.render_row_menu(window, cx);
        let resize = self.sidebar_open.then(|| self.render_resize_handle(window, cx));
        let resize_overlay = self.render_resize_overlay(cx);
        let folder_drag_overlay = self.render_folder_drag_overlay(cx);
        let home_drag_overlay = self.render_home_drag_overlay(window, cx);
        let file_drop = self.render_file_drop(window, cx);
        let create_menu = self.render_create_menu(cx);
        let place_menu = self.render_place_menu(cx);
        let crumb_menu = self.render_crumb_menu(cx);
        let search = self.render_search(window, cx);
        let doc_menu = self.render_doc_menu(window, cx);
        let doc_info = self.render_doc_info(cx);
        let history = self.render_history(cx);
        // a dialog holds the keyboard while open (Esc and ↩ go to it, not to the editor behind)
        self.dialog_focus.sync(self.export_md.is_some() || self.doc_info.is_some() || self.import_apps.is_some(), window, cx);
        let confirm = self.render_confirm(cx);
        let trash_menu = self.render_trash_menu(cx);
        let home_menus = self.render_home_menus(cx);
        let theme_menu = self.render_theme_menu(cx);
        let lang_menu = self.render_lang_menu(cx);
        let pref_menu = self.render_pref_menu(cx);
        let font_menu = self.render_font_menu(cx);
        let toast = self.render_toast(window);
        let forced_update = crate::update::prompt(window, cx);
        let mode_menu = self.render_mode_menu(cx);
        let main = match self.view {
            View::Doc => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(self.top_bar(f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. }, cx))
                .children(self.render_history_banner(cx))
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
            View::Home => self.render_home(window, cx),
            View::Folder => self.render_folder(window, cx),
            View::Trash => self.render_trash(cx),
            View::Settings => self.render_settings(cx),
        };
        let move_dlg = self.render_move_dialog(cx);
        let export_md = self.render_export_md(cx);
        let import_apps = self.render_import_apps(cx);
        // Only the note stands out: a white plane flush with the window's top,
        // right and bottom edges. The sidebar sits sunken on the window
        // surface beside it; a faint shade along the plane's left edge parts
        // the two (no gutter, radius or border).
        // A document sits on its theme's page colour.
        let page = if self.view == View::Doc { crate::editor_theme::Theme::bg_body() } else { Theme::bg_body() };
        // The first listing still running, or fading out (`loading`): its layer over 主页 or a folder's page.
        crate::loading::drawn(cx);
        let load = crate::loading::phase(cx);
        if load.moving() { window.request_animation_frame(); }
        let cover = crate::loading::cover(cx);
        let covers = cover > 0. && if load.loading() { matches!(self.view, View::Home | View::Folder) } else { self.load_cover && load.moving() };
        let load_layer = covers.then(|| {
            let counts = cx.try_global::<crate::loading::Load>().map(|l| l.counts()).unwrap_or_default();
            // A Markdown file let go here opens where it is, as from the Finder.
            crate::loading::note_layer(load, cover, crate::loading::t(cx), counts)
                .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| w.file_drop_paths(paths.paths().to_vec(), window, cx)))
        });
        let note = div().id("note").relative().flex_1().min_w_0().h_full().pt(px(inset_top)).flex().bg(page).shadow(theme::shadow_note()).child(main)
            // Files from the Finder dropped on 主页 or a folder's page are imported there.
            .when(matches!(self.view, View::Home | View::Folder), |d| d
                .on_drag_move(cx.listener(|w, ev: &DragMoveEvent<ExternalPaths>, _, cx| w.file_drag_move(ev, cx)))
                .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| w.file_dropped(paths, window, cx))))
            .children(file_drop)
            .children(load_layer)
            .children(history);
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
            .pb(px(keyboard))
            .flex()
            .bg(Theme::window())
            .font_family(theme::ui_family())
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
            .on_action(cx.listener(|w, _: &ReplayOnboarding, window, cx| w.replay_onboarding(window, cx)))
            .on_action(cx.listener(|w, _: &OpenAbout, _, cx| w.open_settings(crate::settings::Page::About, cx)))
            .on_action(cx.listener(|w, _: &CheckForUpdates, _, cx| { w.open_settings(crate::settings::Page::About, cx); crate::update::check(true, cx); }))
            .on_action(cx.listener(|w, _: &GoHome, _, cx| w.show_view(View::Home, cx)))
            .on_action(cx.listener(|w, _: &NewDoc, window, cx| w.new_doc(window, cx)))
            .on_action(cx.listener(|w, _: &ImportDoc, window, cx| w.open_import(window, cx)))
            .on_action(cx.listener(|w, _: &OpenFile, window, cx| w.open_file_dialog(window, cx)))
            .on_action(cx.listener(|w, _: &OpenFolder, window, cx| w.open_folder_dialog(window, cx)))
            .on_action(cx.listener(|w, _: &ImportFolder, window, cx| w.open_import_folder(window, cx)))
            .on_action(cx.listener(|w, _: &ImportFromApps, window, cx| w.open_import_apps(None, window, cx)))
            .on_action(cx.listener(|w, _: &ToggleSidebar, _, cx| {
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
                if (w.theme_menu.is_some() && w.theme_menu_key(&ev.keystroke.key, cx)) || (w.lang_menu.is_some() && w.lang_menu_key(&ev.keystroke.key, cx))
                    || (w.pref_menu.is_some() && w.pref_menu_key(&ev.keystroke.key, cx)) {
                    cx.stop_propagation();
                    return;
                }
                // Esc leaves 设置 (unless its filter field has the caret).
                if ev.keystroke.key == "escape" && w.view == View::Settings && !w.settings_filter.read(cx).is_focused(window) {
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
            }))
            // Esc leaves 批量操作 (closing its menu or confirmation first), even from the editor.
            // Esc closes the document's menus first (Radix's DropdownMenu). The editor binds Esc to its
            // own action, which runs before any key listener, and spends it selecting the block: take
            // the action on its way down while a menu is open (a switch row keeps the menu open, so
            // Esc is how it goes).
            .capture_action(cx.listener(|w, _: &crate::editor::keymap::Escape, _, cx| {
                if w.dismiss_library_view_menu(cx) { cx.stop_propagation(); return; }
                if w.doc_menu || w.mode_menu {
                    w.mode_menu = false;
                    w.doc_menu = false;
                    w.doc_sub = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_key_down(cx.listener(|w, ev: &KeyDownEvent, _, cx| {
                // A required update waits for 立即重启更新 (↩): no key reaches the window beneath.
                if crate::update::blocking(cx) {
                    if ev.keystroke.key == "enter" { crate::update::install(cx); }
                    cx.stop_propagation();
                    return;
                }
                let undo = ev.keystroke.modifiers.secondary(cx) && matches!(ev.keystroke.key.as_str(), "z" | "y");
                if w.store.file_task_busy() || (w.file_history.busy && !undo) { cx.stop_propagation(); return; }
                // 版本历史: the editor keeps the caret, so Esc has to be taken
                // before it reaches the read-only key handler.
                if ev.keystroke.key == "escape" && w.preview.is_some() {
                    w.exit_preview(cx); cx.stop_propagation(); return;
                }
                if ev.keystroke.key == "escape" && w.history.is_some() {
                    w.close_history(cx); cx.stop_propagation(); return;
                }
                if ev.keystroke.key == "escape" && w.has_home_drag() {
                    w.cancel_home_drag(cx); cx.stop_propagation(); return;
                }
                if ev.keystroke.key == "escape" && w.folder_drag.active.is_some() {
                    w.end_folder_drag(true, cx); cx.stop_propagation(); return;
                }
                if w.record_shortcut(ev, cx) { return; }
                // Esc cancels a confirmation dialog.
                if ev.keystroke.key == "escape" && (w.confirm.is_some() || w.export_md.is_some() || w.import_apps.is_some()) {
                    w.confirm = None;
                    w.after_save = None;
                    w.export_md = None;
                    w.import_apps = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if ev.keystroke.key == "escape" && w.dismiss_library_view_menu(cx) {
                    cx.stop_propagation();
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
            .children(sidebar.map(|s| if inset_top + inset_bottom > 0. { div().flex_none().h_full().pt(px(inset_top)).pb(px(if keyboard > 0. { 0. } else { inset_bottom })).child(s).into_any_element() } else { s }))
            .child(note)
            .children(resize)
            .children(crate::ui::window_controls(window))
            .children(row_menu)
            .children(doc_menu)
            .children(mode_menu)
            .children(doc_info)
            .children(trash_menu)
            .children(home_menus)
            .children(theme_menu)
            .children(lang_menu)
            .children(pref_menu)
            .children(font_menu)
            .children(confirm)
            .children(move_dlg)
            .children(export_md)
            .children(import_apps)
            .children(search)
            .children(resize_overlay)
            .children(folder_drag_overlay)
            .when(self.file_history.busy || self.store.file_task_busy(), |d| d.child(deferred(div().id("file-history-pending").absolute().inset_0().occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())).with_priority(20)))
            .children(home_drag_overlay)
            .children(create_menu)
            .children(place_menu)
            .children(crumb_menu)
            .children(forced_update)
            .children(toast)
            // Over everything, toasts included, until its last page is done.
            .when_some(self.onboarding.clone(), |d, o| d.child(deferred(div().id("onboarding-cover").absolute().inset_0().occlude().child(o)).with_priority(30)))
    }
}

impl Workspace {
    /// Lay the first-launch pages over this window; finishing them uncovers it.
    pub(crate) fn replay_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding.is_some() { return }
        let o = cx.new(|cx| crate::onboarding::Onboarding::replay(window, cx));
        cx.subscribe_in(&o, window, |w, _, _: &crate::onboarding::Done, window, cx| {
            w.onboarding = None;
            // The language may have changed on the first page.
            crate::settings::refresh_all(w, cx);
            window.focus(&w.focus, cx);
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

    /// 文件's import group has what the in-window 导入 menus have: 从其他应用导入… (思源笔记,
    /// Obsidian, Typora…) next to 导入文件夹.
    #[test]
    fn the_file_menu_imports_from_other_apps() {
        let file = names(&menu_bar(false));
        let at = |n: &str| file.iter().position(|d| d == n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(at(tr("从其他应用导入…", "Import from Another App…")), at(tr("导入文件夹…", "Import Folder…")) + 1);
        assert!(!file.iter().any(|d| d == "导入思源笔记…" || d == "Import from SiYuan…"), "思源笔记 is one of the apps now");
        assert_eq!(at(tr("导入文件夹…", "Import Folder…")), at(tr("导入为文档…", "Import as Document…")) + 1);
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
