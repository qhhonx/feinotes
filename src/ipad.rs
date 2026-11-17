//! Fei Notes application host using Longbridge's UIKit platform.
use crate::{assets::Assets, editor, settings, shared_store::SharedStore, themes};
use gpui::*;
use gpui_mobile::ios::ffi;
use std::{cell::RefCell, ffi::{CStr, c_char}, path::PathBuf, rc::Rc};

thread_local! {
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

pub(crate) fn has_cloud() -> bool {
    #[cfg(feature = "cloudkit-sync")]
    return crate::ipad_session::has_cloud();
    #[cfg(not(feature = "cloudkit-sync"))]
    false
}

fn open_editor(cx: &mut App) -> anyhow::Result<()> {
    editor::math::load_fonts(cx);
    crate::board::fonts::load(cx);
    crate::board::bind_keys(cx);
    editor::image::init_pictures(cx);
    crate::input::bind_keys(cx);
    editor::keymap::bind_keys(cx);
    editor::image::bind_keys(cx);
    crate::app::bind_keys(cx);
    crate::shortcuts::init(cx);
    settings::load();
    settings::sync_dark(cx);
    #[cfg(feature = "cloudkit-sync")]
    crate::cloud_subscription::watch(cx);
    themes::load(&settings::prefs().theme_id(), false, || cx.text_system().all_font_names());

    let mut store = SharedStore::open()?;
    #[cfg(feature = "mobile-qa")]
    if let Ok(path) = std::env::var("FEI_MOBILE_QA_FIXTURES") {
        let path = std::path::Path::new(&path);
        let samples = path.join("samples");
        let assets = crate::store::Store::assets_in(store.root());
        std::fs::create_dir_all(&assets)?;
        for name in ["image.png","two-pages.pdf","audio-15s.wav","video-15s.mp4","file.txt"] {
            let source = samples.join(name);
            if source.is_file() { std::fs::copy(source,assets.join(format!("qa45-{name}")))?; }
        }
        for entry in std::fs::read_dir(path)?.flatten() {
            if entry.path().extension().and_then(|v| v.to_str()) != Some("xml") { continue; }
            let mut xml = std::fs::read_to_string(entry.path())?;
            for name in ["image.png","two-pages.pdf","audio-15s.wav","video-15s.mp4","file.txt"] {
                xml = xml.replace(&format!("@{name}@"),&crate::store::asset_source_in(store.root(),&assets.join(format!("qa45-{name}"))));
            }
            let doc = crate::docxml::parse(&xml)?;
            if !store.list().iter().any(|e| e.title == doc.title.text) { store.create_from(&doc)?; }
        }
    }
    #[cfg(feature="mobile-qa")]
    if std::env::var_os("FEI_MOBILE_QA_FIXTURES").is_some() {
        let mut target=None;
        for title in ["QA49 Linked target","QA49 Linked middle","QA49 Linked source"] {
            let existing=store.list().into_iter().find(|e|e.title==title);
            let id=if let Some(entry)=existing { entry.id } else {
                let body=target.as_ref().map(|id|format!("<cite type=\"doc\" doc-id=\"{id}\">Open next document</cite>")).unwrap_or_else(||"Synthetic target content.".into());
                store.create_from(&crate::docxml::parse(&format!("<title>{title}</title><p>{body}</p><p>Return to the previous document.</p>"))?)?
            };
            target=Some(id);
        }
    }
    let entry = store.list().into_iter().find(|entry| entry.meta.folder.is_none());
    let id = match entry {
        Some(entry) => Some(entry.id),
        None if has_cloud() => None,
        #[cfg(feature = "cloudkit-sync")]
        None if crate::ipad_session::resuming() => None,
        None => {
            let xml = "<title>随手记</title>\n<p>在这里记下今天的想法。</p>";
            Some(store.create_from(&crate::docxml::parse(xml)?)?)
        }
    };
    cx.set_global(store);
    #[cfg(feature = "cloudkit-sync")]
    crate::ipad_session::start(crate::ipad_session::base_path()?,cx)?;
    cx.open_window(WindowOptions::default(), move |window, cx| {
        cx.new(|cx| {
            let mut workspace = match id {
                Some(id) => crate::app::Workspace::new_document_window(Some(id), window, cx),
                None => crate::app::Workspace::new(window, cx),
            };
            workspace.search.update(cx, |field, _| { field.size = 17.; field.line_height = 24.; });
            if crate::mobile::native_shell(window) { workspace.show_view(crate::app::View::Home, cx); }
            workspace
        })
    })?;
    #[cfg(feature = "cloudkit-sync")]
    crate::ipad_session::resume_when_ready(cx);
    Ok(())
}

/// The UIKit host calls this once, on the main thread, before attaching the view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fei_ipad_start(library: *const c_char) -> bool {
    if library.is_null() { return false; }
    let path = unsafe { CStr::from_ptr(library) };
    let Ok(path) = path.to_str() else { return false; };
    let path = PathBuf::from(path);
    if !path.is_absolute() || !path.ends_with("FeiNotesEditorProbe") { return false; }
    #[cfg(feature = "cloudkit-sync")]
    let path = crate::ipad_session::startup_root(&path);
    if !has_cloud()
        && let Err(error) = std::fs::create_dir_all(&path).map_err(anyhow::Error::from)
            .and_then(|()| crate::store::Store::initialize_local_sqlite(&path)) {
        eprintln!("Fei Notes local library startup: {error:#}");
        return false;
    }
    if crate::store::Store::set_sqlite_session_root(path).is_err() { return false; }
    ffi::gpui_ios_set_embedded();
    if ffi::gpui_ios_initialize().is_null() { return false; }
    let _ = log::set_logger(&PROBE_LOG);
    log::set_max_level(log::LevelFilter::Info);
    let app = Application::with_platform(Rc::new(gpui_mobile::IosPlatform::new()))
        .with_assets(Assets)
        .run_embedded(|cx| {
            if let Err(error) = open_editor(cx) { eprintln!("Fei Notes editor startup: {error:#}"); }
        });
    APPLICATION.with(|slot| *slot.borrow_mut() = Some(app));
    ffi::gpui_ios_did_finish_launching(std::ptr::null_mut());
    !ffi::gpui_ios_get_window().is_null()
}


/// The host calls this on a worker thread, before creating the GPUI app.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fei_ipad_prepare_cloud(_library: *const c_char, _channel: *const c_char) -> bool {
    #[cfg(feature = "cloudkit-sync")]
    {
        if _library.is_null() || _channel.is_null() { return false; }
        let path = unsafe { CStr::from_ptr(_library) }.to_str().map(PathBuf::from);
        let channel = unsafe { CStr::from_ptr(_channel) }.to_str().ok().and_then(|value| uuid::Uuid::parse_str(value).ok());
        let (Ok(path), Some(channel)) = (path, channel) else { return false; };
        if !path.is_absolute() || !path.ends_with("FeiNotesEditorProbe") { return false; }
        match crate::ipad_session::prepare_start(path,channel) {
            Ok(cached) => return cached,
            Err(error) => eprintln!("Fei Notes CloudKit startup: {error:#}"),
        }
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn fei_ipad_flush() {
    APPLICATION.with(|slot| {
        if let Some(app) = slot.borrow().as_ref() {
            let mut canceled_composition = false;
            app.update(|cx| {
                for window in cx.windows().into_iter().filter_map(|window| window.downcast::<crate::app::Workspace>()) {
                    let _ = window.update(cx, |workspace, _, cx| {
                        if let Some(editor) = &workspace.editor {
                            canceled_composition |= editor.update(cx, |editor, cx| editor.cancel_background_composition(cx));
                        }
                        workspace.flush(cx)
                    });
                }
            });
            if canceled_composition {
                ffi::gpui_ios_hide_keyboard(ffi::gpui_ios_get_window());
            }
        }
    });
}

/// Close the embedded app on the main thread before thread-local GPU teardown.
#[unsafe(no_mangle)]
pub extern "C" fn fei_ipad_shutdown() {
    fei_ipad_flush();
    let app = APPLICATION.with(|slot| slot.borrow_mut().take());
    if let Some(app) = app {
        #[cfg(feature = "cloudkit-sync")]
        app.update(crate::cloud_sync::stop);
        drop(app);
    }
}


struct ProbeLog;
static PROBE_LOG: ProbeLog = ProbeLog;
impl log::Log for ProbeLog {
    fn enabled(&self, record: &log::Metadata<'_>) -> bool { record.level() <= log::Level::Info }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}

#[derive(serde::Serialize)]
struct NavigationRoute { key: String, title: String }

#[derive(serde::Serialize)]
struct NavigationItem {
    id: String, title: String, parent: Option<String>, folder: bool, pinned: bool, starred: bool, trashed: bool, shared: bool, owned: bool,
}

#[derive(serde::Serialize)]
struct NativeConfirmation { versions: Option<Vec<NativeCloudVersion>>, selected_version: Option<i64>, serial: u64, title: String, message: String, action: &'static str, danger: bool, choice: Option<&'static str>, choice_on: bool, save_failed: bool }

#[derive(serde::Serialize)]
struct NativeCloudVersion { sequence:i64, time:String, preview:String }

#[derive(serde::Serialize)]
struct NativeListing { kind:&'static str, can_reorder:bool, shared_count:usize, selection:Option<serde_json::Value>, interface_size:String, notice:Option<String>, notice_kind:Option<&'static str>, scope:String, query:String, tab:usize, tabs:bool, empty:String, rows:Vec<NativeListingRow> }
#[derive(serde::Serialize)]
struct NativeListingRow { id:String, title:String, summary:String, folder:bool, pinned:bool }
pub(crate) fn editor_scope(workspace:&crate::app::Workspace) -> String { format!("{:?}|{:?}",workspace.store.root(),workspace.current) }
fn listing_scope(workspace:&crate::app::Workspace) -> String { format!("{:?}|{:?}|{:?}",workspace.store.root(),workspace.view,(workspace.folder_current.clone(),workspace.home.tab)) }

#[derive(serde::Serialize)]
struct NavigationState {
    #[cfg(feature = "mobile-qa")]
    qa:Option<serde_json::Value>,
    browser:Option<serde_json::Value>,
    settings:Option<serde_json::Value>,
    local_history:Option<crate::history::mobile::NativeHistory>,
    editor_scope:String, reading:bool, write_allowed:bool, find_active:bool,
    listing:Option<NativeListing>,
    information:Option<crate::docinfo::NativeInformation>,
    can_reorder:bool,
    reorder:Option<crate::mobile_reorder::ReorderRequest>,
    cloud_history: bool,
    routes: Vec<NavigationRoute>,
    kind: String,
    title: String,
    current: Option<String>,
    busy: bool,
    cloud_active: bool,
    cloud_start_pending: bool,
    shared_session: bool,
    can_create: bool,
    can_edit: bool,
    table_active: bool,
    has_code: bool,
    confirmation: Option<NativeConfirmation>,
    appearance: String,
    creation_ack: u64,
    theme: serde_json::Value,
    items: Vec<NavigationItem>,
    labels: std::collections::BTreeMap<&'static str, &'static str>,
}

#[derive(Default)]
struct PublishedNavigation { revision: u64, payload: Vec<u8>, confirmation: Option<(String,u64)>, creation_ack:u64 }
thread_local! { static NAVIGATION: RefCell<PublishedNavigation> = RefCell::new(PublishedNavigation::default()); }

fn confirmation_key(workspace: &crate::app::Workspace) -> Option<String> {
    workspace.confirm.as_ref().map(|confirm| {
        let trash = matches!(confirm,crate::app::Confirm::EmptyTrash).then(||workspace.trash.iter().map(|entry|&entry.id).collect::<Vec<_>>());
        format!("{:?}|{:?}|{:?}|{:?}|{:?}",crate::store::Store::root_dir(),workspace.current,confirm,workspace.confirmation_text(),trash)
    })
}

fn confirmation_matches(workspace: &crate::app::Workspace, serial: Option<u64>) -> bool {
    let Some(key) = confirmation_key(workspace) else { return false };
    NAVIGATION.with(|published|published.borrow().confirmation.as_ref().is_some_and(|(current,expected)|current == &key && serial == Some(*expected)))
}

#[derive(serde::Serialize)]
struct NavigationEnvelope<'a> { revision: u64, #[serde(flatten)] state: &'a NavigationState }

unsafe extern "C" { pub(crate) fn fei_ios_is_phone() -> bool; }

unsafe extern "C" { fn fei_ios_navigation_changed(json: *const c_char); }

fn native_theme() -> serde_json::Value {
    let style = crate::themes::active();
    let rgba = |color:gpui::Hsla| { let c = color.to_rgb(); [c.r,c.g,c.b,c.a] };
    let optional = |key:&str| style.color(key).map(rgba);
    serde_json::json!({
        "id":crate::settings::prefs().theme_id(),
        "accent":rgba(style.color("mobile.accent").unwrap_or_else(||style.token("primary_500"))),
        "paper":rgba(style.color("page.bg").unwrap_or_else(||style.token("bg_body"))),
        "folder":rgba(style.color("sidebar.folder").unwrap_or_else(||style.token("primary_500"))),
        "chrome":optional("mobile.chrome.bg"),"list":optional("mobile.list.bg"),
        "text":optional("mobile.list.text"),"row":optional("mobile.row.bg"),
        "selected":optional("mobile.row.selected"),"selected_text":optional("mobile.row.selected.text")
    })
}

pub(crate) fn publish_navigation(workspace: &crate::app::Workspace, cx: &App) {
    use crate::{app::View, settings::{tr, display_title}};
    let title = workspace.current.as_ref().and_then(|id| workspace.entries.iter().find(|entry| &entry.id == id))
        .map(|entry| display_title(&entry.title)).unwrap_or_else(|| tr("笔记", "Note").into());
    let mut routes = vec![NavigationRoute { key: "notes".into(), title: tr("主页","Home").into() }];
    let trash_origin=workspace.mobile_navigation.trash.as_ref().filter(|point|point.root==workspace.store.root());
    let in_settings=workspace.view==View::Settings || workspace.view==View::Trash && trash_origin.is_some_and(|point|point.view==View::Settings);
    let previous = if in_settings { workspace.mobile_settings_return } else if workspace.view==View::Trash { trash_origin.map_or(View::Home,|point|point.view) } else { workspace.view };
    let mut ancestors = Vec::new();
    let mut folder = workspace.folder_current.clone();
    if !workspace.is_shared() && workspace.home.tab==crate::home::HomeTab::Shared { for (id,title) in workspace.mobile_collection_path(cx) { routes.push(NavigationRoute {key:format!("folder:{id}"),title}); } }
    while let Some(id) = folder {
        if ancestors.iter().any(|(seen, _): &(String, String)| seen == &id) { break; }
        let Some(entry) = workspace.entries.iter().find(|entry| entry.id == id) else { break };
        ancestors.push((id, display_title(&entry.title)));
        folder = entry.meta.parent.clone();
    }
    if matches!(previous, View::Doc | View::Folder) {
        for (id, title) in ancestors.into_iter().rev() {
            routes.push(NavigationRoute { key: format!("folder:{id}"), title });
        }
    }
    for point in &workspace.mobile_navigation.links {
        if point.root==workspace.store.root() && let Some(entry)=point.current.as_ref().and_then(|id|workspace.entries.iter().find(|e|&e.id==id)) {
            routes.push(NavigationRoute { key:point.key.clone(),title:display_title(&entry.title) });
        }
    }
    if previous == View::Doc && workspace.current.is_some() {
        routes.push(NavigationRoute { key: workspace.mobile_document_key(), title: title.clone() });
    } else if previous == View::Doc && workspace.draft.is_some() {
        routes.push(NavigationRoute { key: "note:draft".into(), title: title.clone() });
    }
    if in_settings {
        routes.push(NavigationRoute { key: "settings".into(), title: tr("设置", "Settings").into() });
        if !workspace.mobile_settings_root {
            if let Some(parent) = workspace.mobile_settings_parent { routes.push(NavigationRoute { key:format!("settings:{parent:?}"),title:parent.title().into() }); }
            routes.push(NavigationRoute { key: format!("settings:{:?}",workspace.settings_page), title: workspace.settings_page.title().into() });
        }
    }
    if workspace.view == View::Trash {
        routes.push(NavigationRoute { key: "trash".into(), title: tr("回收站", "Trash").into() });
    }
    let kind = match workspace.view { View::Doc => "note", View::Settings => "settings", View::Folder => "folder", View::Trash => "trash", View::Home | View::Shared => "notes" };
    let local_history = workspace.native_history(cx);
    let kind = if workspace.view == View::Doc && local_history.is_some() { if workspace.preview.is_some() { "history-preview" } else { "history" } } else { kind };
    let title = if let Some(history) = &local_history {
        routes.push(NavigationRoute { key:format!("history:{}",history.token),title:tr("版本历史","Version history").into() });
        if workspace.preview.is_some() {
            let label = history.versions.iter().find(|v|Some(&v.name) == history.selected.as_ref()).map(|v|v.title.clone()).unwrap_or_else(||tr("预览版本","Preview version").into());
            routes.push(NavigationRoute { key:format!("history-preview:{}",history.token),title:label.clone() }); label
        } else { tr("版本历史","Version history").into() }
    } else { title };
    let labels = [
        ("folders",tr("文件夹","Folders")),("select-note",tr("选择一篇文档","Select a document")),("show-browser",tr("显示文档列表","Show documents")),("focus-note",tr("专注正文","Focus on note")),
        ("history",tr("版本历史","Version history")),("history-save",tr("保存当前版本","Save current version")),("history-named",tr("仅看命名版本","Named versions only")),("history-marks",tr("显示改动","Show changes")),("history-return",tr("返回当前正文","Return to current note")),("history-restore",tr("恢复此版本","Restore this version")),("history-copy",tr("另存为笔记","Save as a note")),("history-name",tr("命名版本","Name version")),("history-delete",tr("删除此版本","Delete version")),("history-filter",tr("筛选版本","Filter versions")),("history-restore-confirm",tr("恢复此版本并替换当前正文？","Restore this version and replace the current note?")),("history-delete-confirm",tr("删除这个本机历史版本？","Delete this local version?")),
        ("doc-info",tr("文档信息","Document information")),("reorder",tr("调整顺序","Reorder")),("cloud-versions",tr("iCloud 历史版本","iCloud Version History")),("sharing",tr("共享文档","Shared documents")),("share-note",tr("共享这篇文档","Share this document")),("return-personal",tr("返回我的文档库","Return to my library")),
        ("edit-note",tr("编辑文档","Edit document")),("keyboard",tr("显示键盘","Show keyboard")),("hide-keyboard",tr("收起键盘","Hide keyboard")),("not-editable",tr("此共享文档当前不可编辑","This shared document is not editable")),("opening",tr("正在打开文档库…","Opening library…")),("search",tr("搜索文档","Search documents")),("all-notes",tr("我的文档","My documents")),("favorites",tr("收藏","Favorites")),("back",tr("返回","Back")),("done",tr("完成","Done")),("settings",tr("设置","Settings")),
        ("note-count",tr("%d 篇文档","%d documents")),("item-count",tr("%d 个项目","%d items")),("result-count",tr("%d 个结果","%d results")),
        ("new",tr("新建文档","New document")),("new-folder",tr("新建文件夹","New folder")),
        ("more",tr("更多","More")),("note-more",tr("文档更多操作","Document actions")),("folder-more",tr("文件夹更多操作","Folder actions")),("end-typing",tr("结束输入","End typing")),("cancel",tr("取消","Cancel")),("rename",tr("重命名","Rename")),
        ("objects",tr("内容设置","Content settings")),("find",tr("文档内查找","Find in document")),("export",tr("导出 Markdown","Export Markdown")),
        ("pdf",tr("导出 PDF","Export PDF")),("print",tr("打印","Print")),
        ("delete",tr("删除文档","Delete document")),("import",tr("导入文档","Import documents")),
        ("new-document",tr("新建文档","New document")),("new-markdown",tr("新建 Markdown","New Markdown")),("batch",tr("选择文档","Select documents")),("sort",tr("排序","Sort")),("manual",tr("手动排序","Manual order")),
        ("visited",tr("最近访问","Recently opened")),("modified",tr("最近修改","Recently modified")),
        ("created",tr("最近创建","Recently created")),("title",tr("标题升序","Title ascending")),
        ("save",tr("保存","Save")),("format",tr("编辑和格式","Editing and formatting")),
        ("options",tr("文档选项","Document options")),("move",tr("移动到","Move to")),
        ("library",tr("文档库","Library")),("library-local",tr("本机文档库","Local library")),
        ("library-cloud",tr("iCloud 文档库","iCloud library")),("library-copy",tr("复制本机笔记并开启 iCloud","Copy local notes and enable iCloud")),
        ("root",tr("我的文档","My documents")),("select-all",tr("全选","Select all")),("deselect-all",tr("取消全选","Deselect all")),("selected-count",tr("已选择 %d 项","%d selected")),("shared-count",tr("%d 篇共享文档","%d shared documents")),("pinned",tr("置顶","Pinned")),("pin",tr("置顶","Pin")),("unpin",tr("取消置顶","Unpin")),
        ("star",tr("收藏","Favorite")),("unstar",tr("取消收藏","Remove favorite")),
        ("duplicate",tr("创建副本","Duplicate")),("text-only",tr("仅文本","Text only")),
        ("all-content",tr("包含图片和附件","Include images and attachments")),
        ("trash",tr("回收站","Trash")),("backup",tr("导出文档库备份","Export library backup")),
        ("restore-backup",tr("从备份恢复","Restore from backup")),
        ("reading",tr("阅读模式","Reading mode")),("editing",tr("编辑模式","Editing mode")),
        ("bold",tr("粗体","Bold")),("italic",tr("斜体","Italic")),("underline",tr("下划线","Underline")),
        ("strike",tr("删除线","Strikethrough")),("inline-code",tr("行内代码","Inline code")),
        ("text",tr("正文","Text")),("heading1",tr("一级标题","Heading 1")),("heading2",tr("二级标题","Heading 2")),
        ("heading3",tr("三级标题","Heading 3")),("bullet",tr("无序列表","Bullet list")),
        ("ordered",tr("有序列表","Numbered list")),("todo",tr("待办列表","Checklist")),
        ("code-tools",tr("代码块工具","Code tools")),
        ("code",tr("代码块","Code block")),("quote",tr("引用","Quote")),
        ("divider",tr("分隔线","Divider")),("table",tr("表格","Table")),
        ("table-controls",tr("表格操作","Table controls")),
        ("outline",tr("文档目录","Outline")),
        ("insert",tr("插入内容","Insert content")),
        ("attachments",tr("图片和附件","Images and attachments")),
        ("new-personal-folder",tr("新建个人文件夹","New personal folder")),("remove-personal-folder",tr("移除分类并保留文档","Remove folder, keep items")),("accept-invitation",tr("接受邀请","Accept invitation")),("delete-shared",tr("删除共享文档","Delete shared document")),("leave-shared",tr("离开共享","Leave share")),
        ("undo",tr("撤销","Undo")),("redo",tr("重做","Redo")),
        ("format-text",tr("文字样式","Text style")),("format-block",tr("段落类型","Block type")),
        ("restore",tr("恢复","Restore")),("purge",tr("彻底删除","Delete permanently")),
        ("empty-trash",tr("清空","Empty")),
    ].into_iter().collect();
    let mut navigation_entries = workspace.personal_navigation_entries(cx);
    navigation_entries.extend(workspace.shared_entries(cx));
    navigation_entries.extend(workspace.mobile_collection_entries(cx));
    if workspace.is_shared() { for entry in &workspace.entries { if !navigation_entries.iter().any(|e|e.id==entry.id) { navigation_entries.push(entry.clone()); } } }
    let mut items: Vec<_> = navigation_entries.iter().map(|entry| NavigationItem {
        id: entry.id.clone(), title: display_title(&entry.title), parent: entry.meta.parent.clone(),
        folder: entry.meta.folder.is_some(), pinned: entry.meta.pinned, starred: entry.meta.starred > 0, trashed: false, shared:workspace.shared_list_room(&entry.id,cx).is_some(), owned:workspace.shared_list_room(&entry.id,cx).is_some_and(|room|room.scope.owner.is_none()),
    }).collect();
    if workspace.view == View::Trash {
        items.extend(workspace.trash.iter().map(|entry| NavigationItem {
            id: entry.id.clone(), title: display_title(&entry.title), parent: entry.parent.clone(),
            folder: false, pinned: false, starred: false, trashed: true, shared:false, owned:false,
        }));
    }
    let confirmation = workspace.confirmation_text().map(|(title,message,action,danger)| {
        use crate::app::Confirm;
        let (choice,choice_on) = match &workspace.confirm {
            Some(Confirm::Refs { show:false,no_ask })=>(Some(tr("不再提示","Don’t ask again")),*no_ask),
            #[cfg(feature = "cloudkit-sync")]
            Some(Confirm::SwitchCloudLibrary { cloud:true,copy_local })=>(Some(tr("同时复制本机笔记到 iCloud","Also copy local notes to iCloud")),*copy_local),
            _=>(None,false),
        };
        let (versions,selected_version) = match &workspace.confirm {
            #[cfg(feature = "cloudkit-sync")]
            Some(Confirm::CloudVersions { versions,selected,.. })=>(Some(versions.iter().map(|(sequence,time,preview)|NativeCloudVersion { sequence:*sequence,time:time.clone(),preview:preview.clone() }).collect()),*selected),
            _=>(None,None),
        };
        NativeConfirmation { versions,selected_version,serial:0,title,message,action,danger,choice,choice_on,save_failed:matches!(workspace.confirm,Some(Confirm::SaveFailed)) }
    });
    let query = workspace.query(cx);
    let listing = ((matches!(workspace.view,View::Home | View::Folder) && unsafe { fei_ios_is_phone() } || workspace.view==View::Trash) && (!workspace.is_shared() || crate::cloud_sync::shared::scope(&workspace.store,cx).is_some_and(|scope|scope.folder))).then(||NativeListing {
        kind:if workspace.view==View::Trash { "trash" } else if !workspace.is_shared() && workspace.home.tab==crate::home::HomeTab::Shared { "shared" } else { "notes" },can_reorder:workspace.mobile_can_reorder(cx),
        shared_count:workspace.shared_entries(cx).len(),selection:workspace.mobile_batch_state(cx),
        notice_kind:workspace.toast.as_ref().map(|(kind,_,_)|match kind { crate::ui::ToastKind::Success=>"success",crate::ui::ToastKind::Error=>"error",crate::ui::ToastKind::Info=>"info" }),
        notice:workspace.toast.as_ref().filter(|(_,_,time)|time.elapsed().as_secs_f32() < 3.).map(|(_,text,_)|text.to_string()),
        interface_size:settings::prefs().ui_font,scope:listing_scope(workspace),query:if workspace.view==View::Trash { String::new() } else { workspace.search.read(cx).text.clone() },tab:match workspace.home.tab { crate::home::HomeTab::Starred=>1,crate::home::HomeTab::Shared=>2,_=>0 },
        tabs:workspace.view == View::Home && query.is_empty(),
        empty:if workspace.view==View::Trash { tr("回收站为空","Trash is empty") } else if !query.is_empty() { tr("没有找到匹配的结果","No matching documents") } else if workspace.home.tab == crate::home::HomeTab::Starred { tr("暂无收藏文档","No favorite documents yet") } else if workspace.home.tab == crate::home::HomeTab::Shared { tr("暂无共享文档，可从菜单接受邀请，或邀请他人协作笔记。","No shared documents. Accept an invitation from the menu, or invite someone to a note.") } else { tr("点新建按钮开始记录","Tap New to start a document") }.into(),
        rows:if workspace.view==View::Trash { workspace.trash.iter().map(|entry|NativeListingRow { id:entry.id.clone(),title:display_title(&entry.title),summary:crate::trash::remaining(entry.deleted),folder:false,pinned:false }).collect() } else { workspace.mobile_visible_entries(cx).into_iter().map(|entry|NativeListingRow {
            id:entry.id.clone(),title:display_title(&entry.title),folder:entry.meta.folder.is_some(),pinned:entry.meta.pinned,
            summary:if entry.meta.folder.is_some() { tr("文件夹","Folder").into() } else { crate::mobile::list_summary(&entry.body,&query) },
        }).collect() },
    });
    let mut state = NavigationState {
        #[cfg(feature = "mobile-qa")]
        qa:workspace.editor.as_ref().map(|ed| { let ed=ed.read(cx);serde_json::json!({"draft":workspace.draft.is_some(),"generation":ed.core.generation,"title":ed.core.doc.title.text,"body":ed.core.doc.plain_text(),"selection":ed.core.sel,"scroll":ed.mobile_scroll_probe()}) }),
        browser:crate::ipad_browser::project(workspace,cx),settings:workspace.native_settings(cx),local_history,editor_scope:editor_scope(workspace),reading:workspace.reading(),write_allowed:crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx),find_active:workspace.editor.as_ref().is_some_and(|ed|ed.read(cx).find.is_some()),listing, information:workspace.native_document_information(),can_reorder:workspace.mobile_can_reorder(cx),reorder:workspace.mobile_reorder_request(cx),cloud_history:crate::cloud_sync::history_available(workspace.store.root(),cx),routes,kind:kind.into(),title,labels,items,
        has_code: workspace.editor.as_ref().is_some_and(|ed| ed.read(cx).has_code_blocks()),
        table_active: workspace.editor.as_ref().is_some_and(|ed| ed.read(cx).mobile_table_target().is_some()),
        current: workspace.current.clone(), busy: workspace.library_switching || workspace.store.file_task_busy() || workspace.file_history.busy || workspace.sharing_busy(), cloud_active: has_cloud(), cloud_start_pending: crate::ipad_session::resuming(), shared_session: workspace.is_shared(), can_create: !workspace.is_shared() || workspace.can_create_in_shared_folder(workspace.creation_parent().as_deref(),cx),
        can_edit: crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx) && workspace.editor.as_ref().is_none_or(|ed| !ed.read(cx).read_only), confirmation, appearance: crate::settings::prefs().appearance, creation_ack:NAVIGATION.with(|p|p.borrow().creation_ack), theme: native_theme() };
    NAVIGATION.with(|published| {
        let Ok(payload) = serde_json::to_vec(&state) else { return };
        let mut published = published.borrow_mut();
        if let Some(key) = confirmation_key(workspace) {
            let serial = published.confirmation.as_ref().filter(|(previous,_)|previous == &key).map(|(_,serial)|*serial).unwrap_or(published.revision+1);
            published.confirmation = Some((key,serial));
            if let Some(confirmation) = &mut state.confirmation { confirmation.serial = serial; }
        } else { published.confirmation = None; }
        if published.payload == payload { return; }
        published.revision += 1;
        published.payload = payload;
        let envelope = NavigationEnvelope { revision: published.revision,state: &state };
        if let Some(json) = serde_json::to_vec(&envelope).ok().and_then(|bytes| std::ffi::CString::new(bytes).ok()) {
            unsafe { fei_ios_navigation_changed(json.as_ptr()); }
        }
    });
}

#[derive(serde::Deserialize)]
struct NativeCommand { scope:Option<String>, action: String, value: Option<String>, id: Option<String>, selection_serial: Option<u64> }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fei_ios_command(json: *const c_char) {
    if json.is_null() { return; }
    let Ok(command) = serde_json::from_slice::<NativeCommand>(unsafe { CStr::from_ptr(json) }.to_bytes()) else { return };
    APPLICATION.with(|slot| {
        if let Some(app) = slot.borrow().as_ref() {
            app.update(|cx| {
                for handle in cx.windows().into_iter().filter_map(|window| window.downcast::<crate::app::Workspace>()) {
                    let _ = handle.update(cx, |workspace, window, cx| {
                        if workspace.store.file_task_busy() || workspace.file_history.busy { return; }
                        if command.action == "settings-command" {
                            if let (Some(id),Some(scope)) = (command.id.as_deref(),command.scope.as_deref()) { workspace.native_settings_command(id,command.value.as_deref().unwrap_or(""),scope,window,cx); }
                            publish_navigation(workspace,cx);
                            return;
                        }
                        #[cfg(feature="mobile-qa")]
                        if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(workspace.store.root().join(".qa49-commands.log")) {
                            let _=std::io::Write::write_all(&mut file,format!("{} view={:?} current={:?}\n",command.action,workspace.view,workspace.current).as_bytes());
                        }
                        let id = command.id.clone().or_else(|| workspace.current.clone());

                        if let Some(room) = crate::cloud_sync::shared::scope(&workspace.store,cx).cloned() {
                            if matches!(command.action.as_str(),"format" | "editing") && !crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx) { return; }
                            let _ = room;
                            if matches!(command.action.as_str(),"new" | "new-document" | "new-markdown" | "new-folder" | "import" | "delete" | "move" | "pin" | "star" | "duplicate" | "backup" | "restore-backup" | "share-note" | "library-copy" | "restore" | "purge" | "empty-trash") { return; }
                        }
                        match command.action.as_str() {
                        "history-open"=>{
                            if workspace.view == crate::app::View::Doc && id == workspace.current && workspace.history.is_none() && command.scope.as_deref() == Some(editor_scope(workspace).as_str()) {
                                if let Some(id) = &id {
                                    if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx|ed.cancel_background_composition(cx)); }
                                    workspace.open_history(id,window,cx);
                                    workspace.exit_preview(cx);
                                    if let Some(h) = &mut workspace.history { h.selected = None; }
                                    ffi::gpui_ios_hide_keyboard(ffi::gpui_ios_get_window());
                                    cx.notify();
                                }
                            }
                        }
                        "history-command"=>{
                            if id == workspace.current { workspace.mobile_history_command(command.selection_serial,command.scope.as_deref(),command.value.as_deref(),window,cx); }
                        }
                        "cloud-versions"=>{
                            if !workspace.library_switching && crate::cloud_sync::history_available(workspace.store.root(),cx) && workspace.view == crate::app::View::Doc && id == workspace.current && workspace.flush(cx) {
                                if let Some(id) = &id { workspace.recover_cloud_version(id.clone(),cx); }
                            }
                        }
                        "cloud-version-select"=>{
                            if workspace.library_switching || !crate::cloud_sync::history_available(workspace.store.root(),cx) || !confirmation_matches(workspace,command.selection_serial) { return; }
                            if let (Some(crate::app::Confirm::CloudVersions { versions,selected,.. }),Some(sequence)) = (&mut workspace.confirm,command.value.as_deref().and_then(|v|v.parse::<i64>().ok())) {
                                if versions.iter().any(|(id,_,_)|*id == sequence) { *selected = Some(sequence); cx.notify(); }
                            }
                        }
                        "reminder-open"=>{
                            if command.value.as_deref() == Some(crate::ipad_notifications::namespace().as_str()) && id.as_ref().is_some_and(|id| workspace.entries.iter().any(|entry|&entry.id == id)) {
                                workspace.open(id.as_deref().unwrap(),window,cx);
                            } else { workspace.show_toast(crate::settings::tr("请先打开该提醒所属的文档库。","Open the library that owns this reminder first."),cx); }
                        }
                        "reminder-settings"=>{ crate::ipad_notifications::publish(workspace,cx); crate::ipad_notifications::present_settings(); },
                        "reminder-refresh"=>crate::ipad_notifications::publish(workspace,cx),
                        "find-query" | "find-next" | "find-previous" | "find-replace" | "find-replace-all" | "find-close" | "find-mode-search" | "find-mode-replace" => {
                            if let (Some(serial),Some(editor)) = (command.selection_serial,workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx|ed.mobile_find_command(serial,&command.action,command.value.as_deref(),window,cx));
                            }
                        }
                        "confirm-action" | "confirm-option" | "cancel-confirm" | "confirm-save-copy" | "confirm-reload" => {
                            if !confirmation_matches(workspace,command.selection_serial) { return; }
                            if command.action == "confirm-action" && matches!(&workspace.confirm,Some(crate::app::Confirm::CloudVersions { .. }))
                                && (workspace.library_switching || !crate::cloud_sync::history_available(workspace.store.root(),cx) || matches!(&workspace.confirm,Some(crate::app::Confirm::CloudVersions { selected:None,.. }))) { return; }
                            NAVIGATION.with(|published|published.borrow_mut().confirmation = None);
                            match command.action.as_str() {
                                "confirm-action"=>{
                                    if matches!(&workspace.confirm,Some(crate::app::Confirm::CloudVersions { selected:None,.. })) { return; }
                                    workspace.confirm_current(window,cx);
                                },
                                "cancel-confirm"=>{ workspace.confirm = None; workspace.after_save = None; cx.notify(); }
                                "confirm-option"=>{
                                    match &mut workspace.confirm {
                                        Some(crate::app::Confirm::Refs { show:false,no_ask })=>*no_ask = !*no_ask,
                                        #[cfg(feature = "cloudkit-sync")]
                                        Some(crate::app::Confirm::SwitchCloudLibrary { cloud:true,copy_local })=>*copy_local = !*copy_local,
                                        _=>return,
                                    }
                                    cx.notify();
                                }
                                "confirm-save-copy" if matches!(workspace.confirm,Some(crate::app::Confirm::SaveFailed))=>workspace.save_failed_copy(window,cx),
                                "confirm-reload" if matches!(workspace.confirm,Some(crate::app::Confirm::SaveFailed))=>{ workspace.confirm = None; workspace.resolve_save_failed(false,window,cx); }
                                _=>{}
                            }
                        }
                        "objects-menu" => {
                            if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx|ed.present_object_list(cx)); }
                        }
                        "object-open" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx|ed.mobile_object_open(serial,name,window,cx));
                            }
                        }
                        "object-edit" | "object-submit" | "object-close" | "object-action" => {
                            if let (Some(serial),Some(editor)) = (command.selection_serial,workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx|ed.mobile_object_command(serial,&command.action,command.value.as_deref(),window,cx));
                            }
                        }
                        "attachments-menu" => {
                            if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx| ed.present_attachment_controls(cx)); }
                        }
                        "attachment-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_attachment_action(serial,name,cx));
                            }
                        }
                        "insert-menu" => {
                            workspace.set_mode(false,window,cx);
                            if let Some(editor)=&workspace.editor { editor.update(cx,|ed,cx|ed.toggle_mobile_format(cx)); }
                        }
                        "insert-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_insert_action(serial,name,window,cx));
                            }
                        }
                        "link-submit" | "link-cancel" | "link-remove" => {
                            if let (Some(serial),Some(editor)) = (command.selection_serial,workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_link_command(serial,&command.action,command.value.as_deref(),window,cx));
                            }
                        }
                        "outline-menu" => {
                            if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx| ed.present_outline_controls(cx)); }
                        }
                        "outline-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_outline_action(serial,name,cx));
                            }
                        }
                        "format-menu" => {
                            if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx| ed.present_format_controls(command.value.as_deref(),cx)); }
                        }
                        "format-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_format_action(serial,name,window,cx));
                            }
                        }
                        "table-menu" => {
                            if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx| ed.present_table_controls(cx)); }
                        }
                        "table-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.as_ref()) {
                                editor.update(cx,|ed,cx| ed.mobile_table_action(serial,name,window,cx));
                            }
                        }
                        "text-action" => {
                            if let (Some(serial),Some(name),Some(editor)) = (command.selection_serial,command.value.as_deref(),workspace.editor.clone()) {
                                if editor.update(cx,|ed,cx| ed.validate_touch_menu(serial,name,cx)) {
                                    if name == "format" { editor.update(cx,|ed,cx| ed.present_format_controls(None,cx)); }
                                    else { dispatch_editor(workspace,name,window,cx); }
                                    if name == "select-all" { window.on_next_frame(move |_,cx| editor.update(cx,|ed,cx| ed.repeat_touch_menu(cx))); }
                                }
                            }
                        }
                        "return-personal" => workspace.navigate_personal(crate::app::View::Home,None,window,cx),
                        "receive-invitation" => { if let Some(link) = command.value.clone() { workspace.receive_invitation(link,window,cx); } }
                        "accept-invitation" => workspace.open_accept_invitation(window,cx),
                        "shared-list" => {
                            if workspace.is_shared() { workspace.navigate_personal(crate::app::View::Shared,None,window,cx); }
                            else if workspace.save_before(cx,|w,_,cx|w.show_view(crate::app::View::Shared,cx)) { workspace.folder_current = None; workspace.show_view(crate::app::View::Shared,cx); }
                        }
                        "shared-pin"|"shared-star"|"shared-copy"|"shared-end"|"shared-members" => {
                            let expected = command.scope.as_deref();
                            if expected != Some(listing_scope(workspace).as_str()) && expected != Some(crate::ipad_browser::scope(workspace).as_str()) && expected != Some(editor_scope(workspace).as_str()) { return; }
                            if let Some(room) = id.as_deref().and_then(|id|workspace.shared_list_room(id,cx)) {
                                match command.action.as_str() {
                                    "shared-pin"|"shared-star" => workspace.toggle_shared_organization(&room.scope.note,command.action == "shared-pin",cx),
                                    "shared-copy" => workspace.copy_shared_documents(vec![room.scope.note.clone()],cx),
                                    "shared-members" => workspace.open_member_management(room,window,cx),
                                    "shared-end" => workspace.sharing_sheet(crate::sharing::Sheet::End(room),window,cx),
                                    _ => {}
                                }
                            }
                        }
                        "sharing" if !workspace.is_shared() => {
                            if workspace.save_before(cx,|w,_,cx|w.show_view(crate::app::View::Shared,cx)) { workspace.folder_current = None; workspace.show_view(crate::app::View::Shared,cx); }
                        }
                        "sharing" => {
                            workspace.mobile_open_settings(cx);
                            workspace.mobile_settings_root = false;
                            workspace.open_settings(crate::settings::Page::Shared,cx);
                            workspace.mobile_shared_settings(window,cx);
                        }
                        "share-note" => { if let Some(id) = &id {
                            workspace.mobile_open_settings(cx);
                            workspace.mobile_settings_root = false;
                            workspace.create_shared_note(id,window,cx);
                        } }
                        "list-reorder" => {
                            if command.scope.as_deref()==Some(listing_scope(workspace).as_str()) || command.scope.as_deref()==Some(crate::ipad_browser::scope(workspace).as_str()) { if let Some(id)=id { workspace.mobile_move_row(&id,command.value.as_deref(),cx); } }
                        }
                        "list-open" | "list-search" | "list-tab" => {
                            if !workspace.library_switching && matches!(workspace.view,crate::app::View::Home | crate::app::View::Folder) && command.scope.as_deref() == Some(listing_scope(workspace).as_str()) {
                                match command.action.as_str() {
                                    "list-open" => { if let Some(id) = &id { if workspace.mobile_visible_entries(cx).iter().any(|entry|&entry.id == id) { workspace.mobile_open_entry(id,window,cx); } } }
                                    "list-search" => { if let Some(query) = &command.value { workspace.search.update(cx,|field,cx|field.set_text(query,cx)); cx.notify(); } }
                                    "list-tab" if workspace.view == crate::app::View::Home => { workspace.mobile_navigation.links.clear(); workspace.home.tab = match command.value.as_deref() { Some("1")=>crate::home::HomeTab::Starred,Some("2")=>crate::home::HomeTab::Shared,_=>crate::home::HomeTab::Recent }; cx.notify(); }
                                    _ => {}
                                }
                            }
                        }
                        "browse-scope" | "browse-search" | "browse-open" | "browse-new-folder" => crate::ipad_browser::command(workspace,&command.action,command.value.as_deref(),command.id.as_deref(),command.scope.as_deref(),window,cx),
                        "back" => workspace.mobile_back(window,cx),
                        "type-scale" => { themes::touch(); settings::refresh_all(workspace,cx); }
                        "settings" => workspace.mobile_open_settings(cx),
                        "close-settings" => workspace.mobile_close_settings(cx),
                        "compose" | "browse-compose" => {
                            let Some(serial) = command.selection_serial else { return };
                            let accepted = NAVIGATION.with(|p| {
                                let mut p = p.borrow_mut();
                                if serial <= p.creation_ack { false } else { p.creation_ack = serial; true }
                            });
                            if !accepted { return; }
                            if command.action == "browse-compose" {
                                crate::ipad_browser::command(workspace,"browse-compose",None,id.as_deref(),command.scope.as_deref(),window,cx);
                                cx.notify();return;
                            }
                            let valid_scope = match workspace.view {
                                crate::app::View::Home | crate::app::View::Folder => command.scope.as_deref() == Some(listing_scope(workspace).as_str()),
                                crate::app::View::Doc => id == workspace.current && command.scope.as_deref() == Some(editor_scope(workspace).as_str()),
                                _ => false,
                            };
                            if valid_scope && !workspace.library_switching && workspace.confirm.is_none() { workspace.create_doc(false,workspace.creation_parent(),window,cx); }
                            cx.notify();
                        }
                        "collection-new" => workspace.personal_collection(workspace.sharing.collection.clone(),window,cx),
                        "collection-rename" => if let Some(id)=&id { workspace.rename_collection(id.clone(),window,cx); },
                        "collection-remove" => if let Some(id)=&id { workspace.remove_collection(id,cx); },
                        "new" => workspace.create_doc(false,workspace.creation_parent(),window,cx),
                        "new-document" => workspace.create_doc(false,workspace.creation_parent(),window,cx),
                        "new-folder" => {
                            let parent=command.id.clone().or_else(||workspace.creation_parent());
                            if parent.as_ref().is_some_and(|id|!workspace.entries.iter().any(|e|&e.id==id && e.meta.folder.is_some())) { return; }
                            workspace.new_folder(parent,window,cx);
                            if let Some(value) = &command.value {
                                if let Some(rename) = &workspace.rename { rename.field.update(cx, |field,cx| field.set_text(value,cx)); }
                                workspace.finish_rename(true,window,cx);
                            }
                        },
                        "doc-info"=>{
                            if !workspace.library_switching && workspace.view == crate::app::View::Doc && id == workspace.current && workspace.confirm.is_none() { workspace.open_doc_info(window,cx); }
                        }
                        "doc-info-close"=>{ if let Some(serial) = command.selection_serial { workspace.close_native_information(serial,cx); } }
                        "reorder-open" => workspace.open_mobile_reorder(cx),
                        "reorder-move" | "reorder-close" => {
                            if let Some(serial) = command.selection_serial { workspace.mobile_reorder_command(serial,&command.action,command.id.as_deref(),command.value.as_deref(),cx); }
                        }
                        "batch-open" => {
                            if command.scope.as_deref()==Some(listing_scope(workspace).as_str()) || command.scope.as_deref()==Some(crate::ipad_browser::scope(workspace).as_str()) { workspace.open_mobile_batch(cx); }
                        },
                        "batch-action" | "batch-close" | "batch-move" | "batch-move-cancel" => {
                            if let Some(serial) = command.selection_serial { workspace.mobile_batch_command(serial,&command.action,command.value.as_deref(),window,cx); }
                        }
                        "find" => workspace.find_in_doc(false,window,cx),
                        "import" => workspace.open_import(window,cx),
                        "delete" => { if let Some(id) = &id { workspace.ask_delete(id,cx); } }
                        "move" => { if let Some(id) = &id { let parent=command.value.clone().filter(|id| !id.is_empty()); if !workspace.mobile_move_shortcut(id,parent.clone(),cx) { workspace.move_docs(&[id.clone()],parent,cx); } } }
                        "pin" => { if let Some(id) = &id { workspace.toggle_pin(id,cx); } }
                        "star" => { if let Some(id) = &id { workspace.toggle_star(id,cx); } }
                        "duplicate" => { if let Some(id) = &id { workspace.duplicate_doc(id,window,cx); } }
                        "export" => { if let Some(id) = &id { workspace.mobile_export(id, command.value.as_deref() == Some("all"), cx); } }
                        "pdf" => { if let Some(id) = &id { workspace.export_pdf(id,window,cx); } }
                        "print" => { if let Some(id) = &id { workspace.print_doc(id,window,cx); } }
                        "sort" => { if let Some(value) = &command.value { workspace.mobile_sort(value,cx); } }
                        "library-local" => workspace.select_library(crate::cloud_restore::LibrarySelection::Local,cx),
                        "library-cloud" => workspace.select_library(crate::cloud_restore::LibrarySelection::Cloud,cx),
                        "library-copy" => workspace.select_library(crate::cloud_restore::LibrarySelection::CopyToCloud,cx),
                        "trash" => workspace.mobile_open_trash(cx),
                        "restore" => { if let Some(id) = id { workspace.confirm = Some(crate::app::Confirm::Restore(id)); cx.notify(); } }
                        "purge" => { if let Some(id) = id { workspace.confirm = Some(crate::app::Confirm::Purge(id)); cx.notify(); } }
                        "empty-trash" => { workspace.confirm = Some(crate::app::Confirm::EmptyTrash); cx.notify(); }
                        "backup" => mobile_backup(workspace,cx),
                        "restore-backup" => workspace.mobile_choose_backup(window,cx),
                        "reading" | "editing" => { if let Some(id) = &id { if workspace.try_open(id,window,cx) { workspace.set_mode(command.action == "reading",window,cx); } } }
                        "toolbar-command" => {
                            if workspace.library_switching || workspace.view != crate::app::View::Doc || id != workspace.current || command.scope.as_deref() != Some(editor_scope(workspace).as_str()) { return; }
                            match command.value.as_deref() {
                                Some("done") => {
                                    if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx|{ ed.cancel_background_composition(cx); ed.dismiss_mobile_format(cx); }); }
                                    workspace.flush(cx);
                                    ffi::gpui_ios_hide_keyboard(ffi::gpui_ios_get_window());
                                }
                                Some("edit") => workspace.set_mode(false,window,cx),
                                Some("quick-checklist" | "quick-insert") if crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx) => {
                                    workspace.set_mode(false,window,cx);
                                    if command.value.as_deref() == Some("quick-checklist") {
                                        dispatch_editor(workspace,"todo",window,cx);
                                        if workspace.editor.as_ref().is_some_and(|ed|!ed.read(cx).read_only) { gpui_mobile::show_keyboard(); }
                                    }
                                    else if let Some(editor) = &workspace.editor {
                                        editor.update(cx,|ed,cx| { if !ed.read_only { ed.toggle_mobile_format(cx); } });
                                        ffi::gpui_ios_hide_keyboard(ffi::gpui_ios_get_window());
                                    }
                                }
                                Some("keyboard") if crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx) => {
                                    if let Some(editor) = &workspace.editor { if !editor.read(cx).read_only { crate::gpui_compat::focus(window,&editor.read(cx).focus_handle(cx),cx); gpui_mobile::show_keyboard(); } }
                                }
                                Some("undo" | "redo" | "bullet" | "todo" | "table") => { if let Some(value) = &command.value { dispatch_editor(workspace,value,window,cx); } }
                                Some("table-tools") => {
                                    if let Some(editor) = &workspace.editor { editor.update(cx,|ed,cx| ed.present_table_controls(cx)); }
                                }
                                Some("styles" | "insert") if crate::cloud_sync::shared::can_edit_in(workspace.store.root(),cx) => {
                                    if let Some(editor)=&workspace.editor {
                                        crate::gpui_compat::focus(window,&editor.read(cx).focus_handle(cx),cx);
                                        editor.update(cx,|ed,cx|ed.toggle_mobile_format(cx));
                                    }
                                }
                                _ => {}
                            }
                        }
                        "format" => { if let Some(value) = &command.value { dispatch_editor(workspace,value,window,cx); } }
                        "rename" => {
                            if let (Some(id),Some(value)) = (id,command.value.as_ref()) {
                                workspace.start_rename(&id,window,cx);
                                if let Some(rename) = &workspace.rename { rename.field.update(cx, |field,cx| field.set_text(value,cx)); }
                                workspace.finish_rename(true,window,cx);
                            }
                        }
                        _ => {}
                        }
                    });
                }
            });
        }
    });
}


unsafe extern "C" {
    fn fei_ios_present_export(path: *const c_char);
    fn fei_ios_item_actions(id: *const c_char);
    fn fei_ios_edit_theme(json: *const c_char);
}

pub(crate) fn export_folder() -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join("FeiNotesExports").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub(crate) fn present_export(path: &std::path::Path) {
    if let Ok(path) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
        unsafe { fei_ios_present_export(path.as_ptr()); }
    }
}

pub(crate) fn present_item_actions(id: &str) {
    if let Ok(id) = std::ffi::CString::new(id) { unsafe { fei_ios_item_actions(id.as_ptr()); } }
}

pub(crate) fn present_theme_editor(path: &std::path::Path) {
    let request = serde_json::json!({ "path": path, "title": settings::tr("编辑 CSS","Edit CSS"),
        "save": settings::tr("保存","Save"), "cancel": settings::tr("取消","Cancel"),
        "discard": settings::tr("放弃修改","Discard changes"),
        "discardMessage": settings::tr("放弃尚未保存的主题修改？","Discard your unsaved theme changes?"),
        "error": settings::tr("无法保存主题，请重试。","Could not save the theme. Please try again."),
        "changed": settings::tr("主题文件已更新，请关闭后重新打开再编辑。","The theme file changed. Close and reopen it before editing.") });
    if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_edit_theme(json.as_ptr()); } }
}

fn dispatch_editor(workspace: &crate::app::Workspace, name: &str, window: &mut Window, cx: &mut Context<crate::app::Workspace>) {
    use crate::editor::keymap as k;
    if let Some(editor) = &workspace.editor {
        if !matches!(name,"copy"|"select-all") && (editor.read(cx).read_only || !crate::cloud_sync::shared::can_edit_in(&editor.read(cx).library_root,cx)) { return; }
        crate::gpui_compat::focus(window,&editor.read(cx).focus_handle(cx),cx);
        if editor.update(cx,|ed,cx| ed.apply_native_format(name,true,window,cx)) { return; }
    }
    let action: Box<dyn Action> = match name {
        "undo" => Box::new(k::Undo), "redo" => Box::new(k::Redo),
        "copy" => Box::new(k::Copy), "cut" => Box::new(k::Cut), "paste" => Box::new(k::Paste), "select-all" => Box::new(k::SelectAll),
        "divider" => Box::new(k::InsertDivider), "table" => Box::new(k::InsertTable), _ => return,
    };
    if let Some(editor) = &workspace.editor {
        crate::gpui_compat::focus(window,&editor.read(cx).focus_handle(cx),cx);
        window.dispatch_action(action,cx);
        if name == "table" {
            let editor = editor.clone();
            window.on_next_frame(move |_,cx| {
                if !editor.read(cx).read_only && crate::cloud_sync::shared::can_edit_in(&editor.read(cx).library_root,cx) {
                    ffi::gpui_ios_show_keyboard(ffi::gpui_ios_get_window());
                }
            });
        }
    }
}

pub(crate) fn mobile_backup(workspace: &mut crate::app::Workspace, cx: &mut Context<crate::app::Workspace>) {
    #[cfg(feature = "cloudkit-sync")]
    {
        if !workspace.save_before(cx, |workspace,_,cx| mobile_backup(workspace,cx)) { return; }
        let root = crate::store::Store::root_dir();
        let session = crate::store::Store::session_lease(&root);
        cx.spawn(async move |this,cx| {
            let result = cx.background_executor().spawn(async move {
                let _session = session;
                export_folder().and_then(|folder| crate::cloud_sync::backup::create(&root,&folder))
            }).await;
            let _ = this.update(cx, |workspace,cx| match result {
                Ok((path,_)) => present_export(&path),
                Err(error) => workspace.show_toast_kind(crate::ui::ToastKind::Error,&error.to_string(),cx),
            });
        }).detach();
    }
}


impl crate::app::Workspace {
    pub(crate) fn mobile_choose_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library_switching { return; }
        let root = crate::store::Store::root_dir();
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true,directories: false,multiple: false,
            prompt: Some(crate::settings::tr("选择飞记备份","Choose a Fei Notes backup").into()) });
        cx.spawn_in(window,async move |this,cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let current = root.clone();
            let session = crate::store::Store::session_lease(&root);
            let prepared = cx.background_executor().spawn(async move { let _session = session; crate::cloud_sync::backup::prepare(&root,&path) }).await;
            let _ = this.update_in(cx,|workspace,_,cx| {
                if crate::store::Store::root_dir() != current { return; }
                match prepared {
                    Ok(backup) => workspace.confirm = Some(crate::app::Confirm::RestoreLibraryBackup(Box::new(backup))),
                    Err(error) => workspace.show_toast_kind(crate::ui::ToastKind::Error,&error.to_string(),cx),
                }
                cx.notify();
            });
        }).detach();
    }
}
