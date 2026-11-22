//! 更新日志: after an update, a quiet card at the foot of the sidebar lists
//! what this version changed, once. The notes are compiled in (no network);
//! a new library (first launch) never sees it.
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::prelude::*;
use gpui::{div, px, relative, AnyElement, ClickEvent, FontWeight, Window, App};

use crate::theme::Theme;
use crate::ui::icon;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const RADIUS: f32 = 12.;
/// 0.1.11's first note; a library moved this launch says it with its counts instead.
const REAL_FOLDERS: &str = "文档库改为真实文件夹，以标题命名，和侧栏一致";

/// Each version's notes: (version, [(中文, English)]). Only the running
/// version's are shown; a version without notes shows nothing.
const NOTES: &[(&str, &[(&str, &str)])] = &[
    ("0.1.14", &[
        ("查找替换支持正则表达式", "Find and replace supports regular expressions"),
        ("Obsidian 的 [[双链]] 可以点击打开对应的笔记", "Obsidian [[wikilinks]] open the note they point to"),
        ("新增土耳其语界面，阿拉伯语从右往左排版", "Turkish interface; Arabic lays out right to left"),
        ("设置里可以选择界面字体和文档字体", "Choose the interface and document fonts in Settings"),
        ("设置 › 文档库可以清理没有笔记用到的图片和文件", "Settings › Documents clears out images and files no note uses"),
        ("新增「退格键回到标题」开关（设置 › 通用）", "New setting: Backspace returns to title (Settings › General)"),
        ("文档很多时搜索不再卡顿，批量删除和清空回收站更快", "Search keeps up with large libraries; deleting many notes and emptying the Trash are faster"),
        ("Windows 和 Linux：复制为 Markdown 改为 Ctrl+Shift+M", "Windows and Linux: Copy as Markdown is now Ctrl+Shift+M"),
        ("画板支持触控板双指缩放；窗口变窄时收起的侧栏，放宽后自动展开", "Pinch to zoom boards on a trackpad; a sidebar hidden by a narrow window comes back when it widens"),
        ("修复切换文档卡顿、代码块折叠不保存、画板选背景色时崩溃和深色模式下按钮看不清", "Fix lag when switching documents, code block folds not saving, a crash in the board's background picker, and hard-to-read buttons in dark mode"),
    ]),
    ("0.1.13", &[
        ("画板：在文档里插入，或直接打开 .excalidraw 文件", "Boards: add one to a note, or open .excalidraw files"),
        ("版本历史：「…」›「版本历史」查看以前保存的版本", "Version history: … › Version history shows earlier saves"),
        ("新增法语和阿拉伯语界面", "French and Arabic interfaces"),
        ("复制为 Markdown（⌘⇧C），侧栏可复制文件路径", "Copy as Markdown (⌘⇧C); copy a file's path from the sidebar"),
        ("设置 › 文档库可以更换文档库文件夹", "Settings › Documents can switch to another library folder"),
        ("从网页或飞书粘贴的图片会存进文档库，不再因链接过期失效", "Pasted web images are saved into the library, so they never expire"),
        ("大图页面更省内存，导入和删除在后台进行，侧栏滚动更顺", "Less memory on picture-heavy pages, imports and deletes in the background, smoother sidebar"),
    ]),
    ("0.1.12", &[
        ("把文件或「本机文件」直接拖进侧栏的文件夹或两行之间", "Drag files, or Files on this computer, into a sidebar folder or between rows"),
        ("拖入时整个页面都是放置区，导入前先确认", "The whole page is a drop target; importing into the sidebar asks first"),
        ("主页卡片改为连续圆角，悬停时有一圈灰色描边", "Home cards have squircle corners and a grey ring on hover"),
        ("主页和文件夹页的「…」可以在文件管理器中打开所在文件夹", "The … menu on Home and folder pages opens the folder in your file manager"),
        ("面包屑从「我的文档」开始，窄窗口时自动收起", "Breadcrumbs start at My documents and fold in a narrow window"),
        ("窗口最小宽度给文档区留足空间，全屏时侧栏不透明", "The window keeps room for the note; the sidebar is solid in full screen"),
        ("修复在文件夹里新建文档时可能卡死", "Fix a hang when creating a document in a folder"),
    ]),
    ("0.1.11", &[
        (REAL_FOLDERS, "The library is now real folders named by title, matching the sidebar"),
        ("全新的首次启动，可从 Obsidian、Typora 等导入笔记", "A new first launch that can bring notes from Obsidian, Typora and more"),
        ("「打开」就地编辑 Markdown，「导入」复制进文档库", "Open edits Markdown in place; Import copies into the library"),
        ("新增日语、西班牙语和韩语界面", "Japanese, Spanish, and Korean interfaces"),
        ("可设置网页搜索、链接浏览器、窗口背景和界面字号", "Set the web search site, link browser, window background, and interface size"),
        ("文档回到上次的阅读位置", "Documents reopen where you left off"),
        ("Markdown 支持块级公式和全角标点，表格列宽按内容调整", "Markdown: display math and full-width punctuation; table columns fit their content"),
        ("导入 Obsidian 的图片嵌入和 callout 提示块", "Import Obsidian picture embeds and callouts"),
        ("「打开上次使用的文档」快捷键可以修改", "The Open Last Document shortcut can be changed"),
        ("文档列表跨启动缓存，启动更快", "Faster launch: the document list is cached between launches"),
        ("新增原样粘贴（⌘⇧V / Ctrl+Shift+V）", "Paste as written (Cmd+Shift+V / Ctrl+Shift+V)"),
        ("图片更省内存，Markdown 内嵌图片可正常显示", "Pictures use less memory; images embedded in Markdown show"),
        ("代码块可横向滚动，拖选到边缘时自动滚动", "Code blocks scroll sideways; drag-selecting scrolls at the edge"),
        ("修复页面跳动、换行处光标、截图粘贴、输入框按词移动和 PDF 扩展名", "Fix page jumps, the caret at wrapped lines, pasting screenshots, moving by word in text fields, and the PDF file extension"),
        ("Windows：显卡设备丢失后不再崩溃", "Windows: no crash when the graphics device is lost"),
        ("更新只从官方地址经 HTTPS 下载", "Updates download only from the official site over HTTPS"),
    ]),
    ("0.1.10", &[
        ("Linux AppImage 支持应用内检查、下载并重启更新", "Check, download, and restart to update the Linux AppImage in the app"),
        ("新建菜单可选择文档、Markdown 或文件夹，快捷新建支持记住上次类型", "Choose Document, Markdown, or Folder from the New menu; quick creation can remember your last choice"),
        ("宽表格新增可拖动的横向滚动条", "Drag the horizontal scrollbar to navigate wide tables"),
        ("Markdown 表格支持按列对齐，并在保存时保留对齐方式", "Align Markdown table columns and preserve alignment when saving"),
        ("长输入框跟随光标滚动，修复链接等内容超出后看不到光标的问题", "Long input fields scroll with the caret, including link fields"),
        ("调整文件夹按钮对齐，改善新建文件夹后的命名体验", "Align folder buttons and improve naming newly created folders"),
    ]),
    ("0.1.9", &[
        ("macOS：修复 ⌘Q 无法退出的问题，保留退出前的保存检查", "macOS: fix Cmd+Q not quitting, while keeping save checks before exit"),
        ("Windows：修复关闭、最大化和最小化按钮无响应的问题", "Windows: fix unresponsive close, maximize, and minimize buttons"),
        ("修复带背景色的文字选中后看不到选区的问题", "Keep text selection visible over highlight colors"),
    ]),
    ("0.1.8", &[
        ("直接打开并编辑本机 Markdown 文件，导入时保留 .md 格式，支持导入文件夹和拖入文件", "Edit local Markdown files in place, keep imported files as .md, and import folders or drag files in"),
        ("首次启动按系统语言选择中文或英文，保留手动选择的语言", "Choose a supported system language on first launch and keep your manual language choice"),
        ("新增多级文件夹，支持创建、重命名、拖动整理和文件操作撤销", "Organize notes in nested folders: create, rename, drag to move, and undo file operations"),
        ("减少图片、文档预览和渲染缓存的内存占用", "Reduce memory used by images, document previews, and rendering caches"),
        ("调整表格列宽后保持页面位置，不再跳页", "Keep the page in place after resizing table columns"),
        ("Windows：移除侧栏重复的首页按钮", "Windows: remove the duplicate Home button in the sidebar"),
    ]),
    ("0.1.7", &[
        ("退出时检查所有窗口的保存状态，保存冲突支持重新载入和另存副本", "Check every window before quitting; reload or save a copy when saving fails"),
        ("退出时无法保存的内容会保留为恢复副本，下次启动恢复", "Preserve unsaved work as recovery copies when quitting and restore it on next launch"),
        ("Markdown 导出保留行内附件的链接和文件", "Markdown exports include inline attachment links and files"),
        ("导出限制本地文件的读取范围和大小，写盘失败时更可靠", "Safer local-file exports and more reliable handling of write failures"),
        ("Windows：修复小狼毫退格多删字符、回车多换行", "Windows: fix extra deletion and newlines when using the Weasel input method"),
    ]),
    ("0.1.6", &[
        ("点击空白或切回窗口后可以继续输入，回到应用时更流畅", "Keep typing after clicking blank space or returning to a window; faster library refresh"),
        ("宽表格可延伸至页边，竖向滚动不再带动表格横移", "Wide tables use the page margins and stay still during vertical scrolling"),
        ("持续输入时也会及时保存，输入时隐藏块手柄", "Save regularly during continuous typing and hide block handles while writing"),
        ("macOS：全局快速笔记默认快捷键改为 ⌥ Space", "macOS: the default global quick-note shortcut is now Option+Space"),
        ("Windows：支持粘贴截图和图片文件、导出 PDF 和打印，修复按词删除", "Windows: paste screenshots and image files, export PDF and print, and delete words correctly"),
    ]),
    ("0.1.5", &[
        ("应用更名为 Quiet.md", "The app is now called Quiet.md"),
        ("首页新增批量管理：创建副本、移动、删除", "Manage several documents at once from Home: duplicate, move, delete"),
        ("回收站可以一键清空", "Empty the Trash in one step"),
        ("插入菜单的图标可以用方向键移动", "Arrow keys move through the insert menu's icons"),
        ("删除全部文档后不再自动新建空文档", "Deleting every document no longer creates an empty one"),
        ("Windows：快捷键显示为 Ctrl / Alt，启动失败时会提示并记录日志", "Windows: shortcuts read Ctrl / Alt, and a failed launch explains itself and leaves a log"),
    ]),
];

static PENDING: AtomicBool = AtomicBool::new(false);
/// The library just moved to real folders: (documents, folders), until the card is closed.
static MIGRATED: std::sync::Mutex<Option<(usize, usize)>> = std::sync::Mutex::new(None);

/// The library was moved to real folders this launch: say so on the card.
pub fn migrated(r: &crate::store::MigrationReport) { *MIGRATED.lock().unwrap() = Some((r.docs, r.folders)); }

fn notes() -> Option<&'static [(&'static str, &'static str)]> { NOTES.iter().find(|(v, _)| *v == VERSION).map(|(_, n)| *n) }

/// At launch, once preferences and themes are loaded: a library that existed
/// before this launch and was last opened by an older version gets the card.
/// Records this version either way, so it shows only once.
pub fn startup(existed: bool) {
    let mut p = crate::settings::prefs();
    if p.seen_version == VERSION { return }
    let upgraded = existed && (p.seen_version.is_empty() || crate::update::newer(VERSION, &p.seen_version));
    PENDING.store(upgraded && notes().is_some(), Ordering::Relaxed);
    p.seen_version = VERSION.into();
    if let Err(e) = crate::settings::save(&p) { log::warn!("changelog: {e:#}"); }
}

/// The card, while it has not been closed: this version's notes, and the
/// move to real folders when it happened this launch.
pub fn card(on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Option<AnyElement> {
    // the site's live demo is not an update anyone installed
    if cfg!(target_family = "wasm") { return None }
    let notes = notes().filter(|_| PENDING.load(Ordering::Relaxed));
    let moved = *MIGRATED.lock().unwrap();
    if notes.is_none() && moved.is_none() { return None }
    let title: String = if notes.is_some() { crate::localized_format!("已更新到 {}", "Updated to {}", VERSION) } else { crate::settings::tr("文档库已改为真实文件夹", "Your library now uses real folders").into() };
    let line = |text: String| div().w_full().min_w_0().flex().gap(px(6.)).text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_caption())
        .child(div().flex_none().child("·")).child(div().flex_1().min_w_0().child(text));
    let mut list = div().mt(px(6.)).w_full().min_w_0().flex().flex_col().gap(px(4.));
    if let Some((docs, folders)) = moved {
        let s = |n: usize| if n == 1 { "" } else { "s" };
        let lang = crate::settings::lang();
        let text = if cfg!(target_os = "macos") {
            crate::settings::localized(lang, "文档库改为真实文件夹：访达里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）",
                || format!("文档库改为真实文件夹：访达里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）"),
                || format!("The library now uses real folders: Finder shows what the sidebar shows, files named by title ({docs} document{}, {folders} folder{})", s(docs), s(folders)))
        } else if cfg!(target_os = "windows") {
            crate::settings::localized(lang, "文档库改为真实文件夹：文件资源管理器里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）",
                || format!("文档库改为真实文件夹：文件资源管理器里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）"),
                || format!("The library now uses real folders: File Explorer shows what the sidebar shows, files named by title ({docs} document{}, {folders} folder{})", s(docs), s(folders)))
        } else {
            crate::settings::localized(lang, "文档库改为真实文件夹：文件管理器里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）",
                || format!("文档库改为真实文件夹：文件管理器里看到的就是侧栏，文件以标题命名（{docs} 篇文档、{folders} 个文件夹）"),
                || format!("The library now uses real folders: your file manager shows what the sidebar shows, files named by title ({docs} document{}, {folders} folder{})", s(docs), s(folders)))
        };
        list = list.child(line(text));
    }
    for &(zh, en) in notes.unwrap_or_default() {
        if moved.is_some() && zh == REAL_FOLDERS { continue }
        list = list.child(line(crate::settings::tr(zh, en).into()));
    }
    // A continuous-corner white surface; the faint shadow follows its radius.
    // At most half the sidebar; a longer list scrolls inside the card.
    Some(crate::ui::continuous_surface(RADIUS, Theme::bg_body()).id("changelog").flex_none().max_h(relative(0.5)).flex().flex_col().min_w_0().overflow_hidden().mx(px(crate::sidebar::SB_X - 4.)).mb(px(2.)).pl(px(12.)).pr(px(6.)).pt(px(8.)).pb(px(10.))
        .rounded(px(RADIUS)).shadow(crate::theme::shadow_card())
        .child(div().flex_none().w_full().min_w_0().flex().items_center().justify_between()
            .child(div().text_size(px(13.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().id("changelog-close").size(px(20.)).flex().items_center().justify_center().rounded(px(6.)).cursor_pointer()
                .hover(|s| s.bg(Theme::fill_hover())).child(icon("x", 12., Theme::icon()))
                .on_click(move |e, w, cx| { PENDING.store(false, Ordering::Relaxed); *MIGRATED.lock().unwrap() = None; on_close(e, w, cx); })))
        .child(div().id("changelog-list").flex_1().min_h_0().overflow_y_scroll().child(list))
        .into_any_element())
}
