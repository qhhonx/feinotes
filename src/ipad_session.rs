//! Replace the product session inside the existing GPUI Mobile window.
use anyhow::{Context as _, Result, ensure};
use gpui::*;
use std::{ffi::{CStr, CString, c_char}, fs::File, path::PathBuf, sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use crate::{app::{View, Workspace}, cloud_restore::{self, LibrarySelection}, shared_store::SharedStore, store::Store};

#[derive(Clone)]
struct Config { base: PathBuf, channel: uuid::Uuid }
#[derive(Clone)]
pub(crate) struct Cloud { pub parent: PathBuf, pub root: PathBuf, pub channel: uuid::Uuid }

static CONFIG: OnceLock<Config> = OnceLock::new();
static PREPARED: Mutex<Option<Cloud>> = Mutex::new(None);
static LOCAL_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);
static RESUME_CLOUD: AtomicBool = AtomicBool::new(false);
static CLOUD_ACTIVE: AtomicBool = AtomicBool::new(false);
static STARTUP_FALLBACK: AtomicBool = AtomicBool::new(false);

struct Session { config: Config, cloud: Option<Cloud>, _local: Option<File> }
impl Global for Session {}

unsafe extern "C" {
    fn fei_ios_library_selection_changed(cloud: bool, channel: *const c_char);
    fn fei_ios_home_available() -> bool;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fei_ios_configure_library(base: *const c_char, channel: *const c_char) -> bool {
    if base.is_null() || channel.is_null() { return false; }
    let (Ok(base),Ok(channel)) = (unsafe { CStr::from_ptr(base) }.to_str(), unsafe { CStr::from_ptr(channel) }.to_str()) else { return false };
    let (base,Ok(channel)) = (PathBuf::from(base),uuid::Uuid::parse_str(channel)) else { return false };
    if !base.is_absolute() || !base.ends_with("FeiNotesEditorProbe") || !cloud_restore::configuration_matches() { return false; }
    CONFIG.set(Config { base: base.clone(), channel }).is_ok()
        || CONFIG.get().is_some_and(|saved| saved.base == base && saved.channel == channel)
}

pub(crate) fn check_cloud_enable() -> cloud_restore::EnableCheck {
    use cloud_restore::EnableCheck;
    let Some(config) = CONFIG.get() else { return EnableCheck::Unavailable; };
    cloud_restore::check_account(&cloud_restore::parent(&config.base,config.channel),config.channel)
}

#[cfg(feature = "mobile-qa")]
pub(crate) fn sync_badge_fixture() -> bool {
    std::env::var("FEI_QA_SYNC_BADGE").as_deref() == Ok("1") && !cloud_restore::production() && CONFIG.get().is_some_and(|config|
        config.channel.to_string() == "44ecb219-6dec-4b51-ae62-bcd575b86813"
        && cloud_restore::parent(&config.base,config.channel).join("pause-cloud").is_file())
}

#[cfg(all(feature = "mobile-qa", target_abi = "sim"))]
pub(crate) fn offline_invitation_fixture() -> bool {
    std::env::var("FEI_QA_OFFLINE_INVITATION").as_deref() == Ok("1") && sync_badge_fixture()
}

fn local_root(config: &Config) -> Result<PathBuf> {
    let target = config.base.join("local-only");
    if !target.join(crate::store::SQLITE_FILE).is_file() && config.base.join(crate::store::SQLITE_FILE).is_file() {
        if target.exists() { std::fs::remove_dir(&target).context("The local library migration is incomplete")?; }
        let _source = cloud_restore::lock_library(&config.base)?;
        crate::cloud_sync::backup::migrate_local(&config.base,&target)?;
    }
    std::fs::create_dir_all(&target)?;
    Store::initialize_local_sqlite(&target)?;
    Ok(target)
}

#[unsafe(no_mangle)]
pub extern "C" fn fei_ios_prepare_local() -> bool {
    let result = CONFIG.get().context("Missing library configuration").and_then(local_root);
    match result {
        Ok(root) => { *LOCAL_ROOT.lock().expect("local library lock") = Some(root); CLOUD_ACTIVE.store(false,Ordering::Release); true }
        Err(error) => { eprintln!("Fei Notes local library startup: {error:#}"); false }
    }
}

// This preflight is disk-only. Account discovery and first restore begin after
// the local workspace is installed, so they cannot hold up the notes list.
pub(crate) fn prepare_start(base: PathBuf, channel: uuid::Uuid) -> Result<bool> {
    let config = CONFIG.get().context("Missing library configuration")?;
    ensure!(config.base == base && config.channel == channel, "The library configuration changed");
    let parent = cloud_restore::parent(&base,channel);
    if let Ok(Some(root)) = cloud_restore::cached(&parent,channel) {
        *PREPARED.lock().expect("prepared library lock") = Some(Cloud { parent,root,channel });
        CLOUD_ACTIVE.store(true,Ordering::Release);
        return Ok(true);
    }
    RESUME_CLOUD.store(true,Ordering::Release);
    Ok(false)
}

pub(crate) fn resuming() -> bool { RESUME_CLOUD.load(Ordering::Acquire) }

pub(crate) fn resume_when_ready(cx: &mut App) {
    if !resuming() { return; }
    let config = cx.global::<Session>().config.clone();
    let parent = cloud_restore::parent(&config.base,config.channel);
    let source = Store::root_dir();
    let Some(handle) = cx.windows().into_iter().find_map(|w| w.downcast::<Workspace>()) else { return };
    cx.spawn(async move |cx| {
        let channel = config.channel;
        loop {
            let directory = parent.clone();
            let result = cx.background_executor().spawn(async move { cloud_restore::prepare(&directory,channel) }).await;
            let root = match result {
                Ok(root) => root,
                Err(error) => {
                    if RESUME_CLOUD.swap(false,Ordering::AcqRel) {
                        STARTUP_FALLBACK.store(true,Ordering::Release);
                        eprintln!("Fei Notes background iCloud startup: {error:#}");
                        let _ = handle.update(cx,|ws,_,cx| { crate::ipad::publish_navigation(ws,cx); cx.notify(); });
                    }
                    return;
                }
            };
            let checked = Instant::now();
            loop {
                let ready = handle.update(cx,|ws,window,cx| {
                    if !resuming() || has_cloud() || Store::root_dir() != source { return Some(true); }
                    if ws.view != View::Home || ws.home.tab != crate::home::HomeTab::Recent || ws.folder_current.is_some() || ws.draft.is_some() || ws.library_switching || ws.sharing.busy
                        || ws.confirm.is_some() || ws.search_open || !ws.query(cx).is_empty() || ws.mobile_batch.is_some()
                        || ws.mobile_reorder.is_some() || ws.store.file_task_busy() || ws.file_history.busy
                        || !unsafe { fei_ios_home_available() } { return None; }
                    // Recheck the current account if the user has been working in the local library.
                    if checked.elapsed() > Duration::from_secs(5) { return Some(false); }
                    if !ws.flush(cx) { return None; }
                    ws.library_switching = true;
                    crate::ipad::publish_navigation(ws,cx);
                    cx.notify();
                    let cloud = Cloud { parent: parent.clone(),root: root.clone(),channel };
                    begin_transition(Plan { root: root.clone(),cloud: Some(cloud),local: None,restore: None },true,window,cx);
                    Some(true)
                });
                match ready { Ok(Some(true)) | Err(_) => return, Ok(Some(false)) => break, Ok(None) => {} }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        }
    }).detach();
}

pub(crate) fn startup_fallback() -> Option<&'static str> {
    STARTUP_FALLBACK.load(Ordering::Acquire).then(|| crate::settings::tr(
        "暂时无法连接 iCloud，已打开本机文档库。原 iCloud 文档和待上传改动会保留。",
        "iCloud is temporarily unavailable. The local library is open; your iCloud documents and pending changes are kept."))
}

pub(crate) fn request(selection: LibrarySelection, cx: &mut App) {
    if let Some(handle) = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>()) {
        cx.defer(move |cx| { let _ = handle.update(cx,|workspace,window,cx| select(workspace,selection,window,cx)); });
    }
}

pub(crate) fn base_path() -> Result<PathBuf> { Ok(CONFIG.get().context("Missing library configuration")?.base.clone()) }

pub(crate) fn has_cloud() -> bool { CLOUD_ACTIVE.load(Ordering::Acquire) }
pub(crate) fn startup_root(base: &std::path::Path) -> PathBuf {
    PREPARED.lock().expect("prepared library lock").as_ref().map(|cloud| cloud.root.clone())
        .or_else(|| LOCAL_ROOT.lock().expect("local library lock").clone()).unwrap_or_else(|| base.into())
}

pub(crate) fn start(base: PathBuf, cx: &mut App) -> Result<()> {
    if CONFIG.get().is_none() { let _ = CONFIG.set(Config { base: base.clone(), channel: uuid::Uuid::new_v4() }); }
    let config = CONFIG.get().context("Missing library configuration")?.clone();
    ensure!(config.base == base, "The library configuration changed");
    let cloud = PREPARED.lock().expect("prepared library lock").take();
    let local = if cloud.is_none() { Some(cloud_restore::lock_library(&Store::root_dir())?) } else { None };
    if let Some(cloud) = &cloud { crate::cloud_sync::start_personal_session(cx,cloud_restore::scope(cloud.channel),cloud.parent.clone())?; }
    #[cfg(feature = "mobile-qa")]
    if cloud.is_some() && sync_badge_fixture() && let Ok(name) = std::env::var("FEI_QA_PERSONAL_SYNC_STATE")
        && let Some(phase) = crate::cloud_sync::personal_status_fixture(&name) {
        crate::cloud_sync::fixture_status(&Store::root_dir(),phase,cx);
    }
    cx.set_global(Session { config,cloud,_local: local });
    Ok(())
}

struct Plan { root: PathBuf, cloud: Option<Cloud>, local: Option<File>, restore: Option<crate::cloud_sync::backup::Prepared> }

fn sharing_ready(workspace: &mut Workspace, cx: &mut Context<Workspace>) -> bool {
    if !workspace.sharing.busy { return true; }
    workspace.show_toast_kind(crate::ui::ToastKind::Info,crate::settings::tr("请等待当前共享操作完成。", "Wait for the current sharing operation to finish."),cx);
    false
}

pub(crate) fn select(workspace: &mut Workspace, selection: LibrarySelection, window: &mut Window, cx: &mut Context<Workspace>) {
    if workspace.library_switching || workspace.personal_settings_store(cx).is_none() || !sharing_ready(workspace,cx) { return; }
    if !workspace.save_before(cx, move |workspace,window,cx| select(workspace,selection,window,cx)) { return; }
    RESUME_CLOUD.store(false,Ordering::Release);
    let config = cx.global::<Session>().config.clone();
    let source = Store::root_dir();
    workspace.library_switching = true;
    crate::ipad::publish_navigation(workspace,cx);
    cx.notify();
    cx.spawn_in(window,async move |this,cx| {
        let expected = source.clone();
        let prepared = cx.background_executor().spawn(async move {
            if selection == LibrarySelection::Local {
                let root = local_root(&config)?;
                let lock = cloud_restore::lock_library(&root)?;
                Ok(Plan { root, cloud: None, local: Some(lock), restore: None })
            } else {
                let parent = cloud_restore::parent(&config.base,config.channel);
                let root = cloud_restore::prepare_selection(&parent,config.channel)?;
                if selection == LibrarySelection::CopyToCloud { crate::cloud_sync::copy_local_library(&root,&source,cloud_restore::scope(config.channel))?; }
                Ok::<_,anyhow::Error>(Plan { root: root.clone(),cloud: Some(Cloud { parent,root,channel: config.channel }),local: None,restore: None })
            }
        }).await;
        let _ = this.update_in(cx, |workspace,window,cx| match prepared {
            Ok(plan) if Store::root_dir() == expected => {
                if !workspace.flush(cx) { workspace.library_switching = false; cx.notify(); return; }
                begin(plan,window,cx);
            }
            Ok(_) => { workspace.library_switching = false; cx.notify(); }
            Err(error) => {
                workspace.library_switching = false;
                workspace.show_toast_kind(crate::ui::ToastKind::Error,&error.to_string(),cx);
            }
        });
    }).detach();
}

pub(crate) fn open_room(room: crate::cloud_sync::shared::Room, cx: &mut App) {
    if let Some(handle) = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>()) {
        cx.defer(move |cx| { let _ = handle.update(cx,|workspace,window,cx| {
            if workspace.library_switching || !sharing_ready(workspace,cx) { return; }
            let retry = room.clone();
            if !workspace.save_before(cx,move |_,_,cx| open_room(retry.clone(),cx)) { return; }
            let Some(cloud) = cx.global::<Session>().cloud.clone() else {
                workspace.show_toast_kind(crate::ui::ToastKind::Error,crate::settings::tr("请先打开 iCloud 文档库。","Open your iCloud library first."),cx);
                return;
            };
            let prepared = crate::cloud_sync::shared::session_root(&cloud.root,&room.scope.note)
                .and_then(|root| {
                    ensure!(crate::cloud_sync::shared::descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope), "The shared note identity changed");
                    Ok(root)
                });
            match prepared {
                Ok(root) => {
                    workspace.library_switching = true;
                    crate::ipad::publish_navigation(workspace,cx);
                    begin(Plan { root,cloud: Some(cloud),local: None,restore: None },window,cx);
                }
                Err(error) => workspace.show_toast_kind(crate::ui::ToastKind::Error,&error.to_string(),cx),
            }
        }); });
    }
}

pub(crate) fn return_to_personal(cx: &mut App) {
    if !crate::in_shared_note() { return; }
    if let Some(handle) = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>()) {
        cx.defer(move |cx| { let _ = handle.update(cx,|workspace,window,cx| {
            if workspace.library_switching || !sharing_ready(workspace,cx) || !workspace.save_before(cx,|_,_,cx| return_to_personal(cx)) { return; }
            let Some(cloud) = cx.global::<Session>().cloud.clone() else { return };
            workspace.library_switching = true;
            crate::ipad::publish_navigation(workspace,cx);
            begin(Plan { root: cloud.root.clone(),cloud: Some(cloud),local: None,restore: None },window,cx);
        }); });
    }
}

pub(crate) fn restore(workspace: &mut Workspace, prepared: crate::cloud_sync::backup::Prepared, window: &mut Window, cx: &mut Context<Workspace>) {
    if workspace.library_switching || workspace.personal_settings_store(cx).is_none() || !sharing_ready(workspace,cx) { return; }
    let pending = std::rc::Rc::new(std::cell::RefCell::new(Some(prepared)));
    let retry = pending.clone();
    if !workspace.save_before(cx,move |workspace,window,cx| {
        if let Some(prepared) = retry.borrow_mut().take() { restore(workspace,prepared,window,cx); }
    }) { return; }
    let Some(prepared) = pending.borrow_mut().take() else { return };
    let cloud = cx.global::<Session>().cloud.clone();
    workspace.library_switching = true;
    crate::ipad::publish_navigation(workspace,cx);
    begin(Plan { root: Store::root_dir(),cloud,local: None,restore: Some(prepared) },window,cx);
}

struct Opening { message: String }
impl Render for Opening {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().items_center().justify_center().bg(crate::theme::Theme::bg_body())
            .text_color(crate::theme::Theme::text()).text_size(px(17.)).p(px(24.)).child(self.message.clone())
    }
}

fn begin(plan: Plan, window: &mut Window, cx: &mut App) { begin_transition(plan,false,window,cx); }

fn begin_transition(plan: Plan, silent: bool, window: &mut Window, cx: &mut App) {
    let source = Store::root_dir();
    let previous = cx.remove_global::<Session>();
    let config = previous.config.clone();
    let previous_cloud = previous.cloud.clone();
    window.replace_root(cx,|_,_| Opening { message: if silent { String::new() } else { crate::settings::tr("正在打开文档库…","Opening library…").into() } });
    crate::cloud_sync::stop(cx);
    drop(cx.remove_global::<SharedStore>());
    drop(previous);
    crate::editor::image::close_pictures(cx);
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let started = Instant::now();
        loop {
            match Store::session_quiescent(&source) {
                Ok(true) => break,
                Ok(false) if started.elapsed() < Duration::from_secs(5) => cx.background_executor().timer(Duration::from_millis(16)).await,
                result => {
                    let message = result.err().map(|error| error.to_string()).unwrap_or_else(|| "The previous library is still closing. Please try again.".into());
                    let _ = handle.update(cx,|_,window,cx| recover(&source,config,previous_cloud,message,window,cx));
                    return;
                }
            }
        }
        let prepared = cx.background_executor().spawn(async move {
            let mut plan = plan;
            if let Some(backup) = plan.restore.take() {
                backup.schedule(&plan.root)?;
                let lock = crate::cloud_sync::backup::install(&plan.root)?.context("The verified backup could not be installed")?;
                if plan.cloud.is_none() { plan.local = Some(lock); }
            }
            Ok::<_,anyhow::Error>(plan)
        }).await;
        let _ = handle.update(cx,|_,window,cx| {
            let result = prepared.and_then(|plan| activate(plan,&source,config.clone(),window,cx));
            if let Err(error) = result { recover(&source,config,previous_cloud,error.to_string(),window,cx); }
        });
    }).detach();
}

fn activate(plan: Plan, source: &std::path::Path, config: Config, window: &mut Window, cx: &mut App) -> Result<()> {
    Store::switch_sqlite_session_root(source,plan.root.clone())?;
    if plan.cloud.is_some() { crate::cloud_sync::library::commit(&plan.cloud.as_ref().unwrap().root)?; }
    install(Session { config,cloud: plan.cloud,_local: plan.local },window,cx)?;
    STARTUP_FALLBACK.store(false,Ordering::Release);
    RESUME_CLOUD.store(false,Ordering::Release);
    let channel = CString::new(cx.global::<Session>().config.channel.to_string())?;
    unsafe { fei_ios_library_selection_changed(has_cloud(),channel.as_ptr()); }
    Ok(())
}

fn install(session: Session, window: &mut Window, cx: &mut App) -> Result<()> {
    crate::cloud_sync::shared::select_session(&Store::root_dir())?;
    CLOUD_ACTIVE.store(session.cloud.is_some(),Ordering::Release);
    cx.set_global(SharedStore::open()?);
    if let Some(cloud) = &session.cloud {
        if crate::in_shared_note() { crate::cloud_sync::start_shared_session(cx,cloud.parent.clone())?; }
        else { crate::cloud_sync::start_personal_session(cx,cloud_restore::scope(cloud.channel),cloud.parent.clone())?; }
    }
    cx.set_global(session);
    crate::settings::load();
    crate::shortcuts::reload(cx);
    crate::themes::load(&crate::settings::prefs().theme_id(),false,|| cx.text_system().all_font_names());
    crate::editor::image::init_pictures(cx);
    window.replace_root(cx,|window,cx| {
        let mut workspace = if let Some(room) = crate::cloud_sync::shared::active() {
            Workspace::new_document_window(Some(room.note),window,cx)
        } else { Workspace::new(window,cx) };
        workspace.search.update(cx,|field,_| { field.size = 17.; field.line_height = 24.; });
        if !crate::in_shared_note() { workspace.show_view(View::Home,cx); }
        workspace
    });
    Ok(())
}

fn recover(source: &std::path::Path, config: Config, cloud: Option<Cloud>, message: String, window: &mut Window, cx: &mut App) {
    if RESUME_CLOUD.swap(false,Ordering::AcqRel) { STARTUP_FALLBACK.store(true,Ordering::Release); }
    crate::cloud_sync::stop(cx);
    if cx.has_global::<SharedStore>() { drop(cx.remove_global::<SharedStore>()); }
    if cx.has_global::<Session>() { drop(cx.remove_global::<Session>()); }
    let current = Store::root_dir();
    let result = (|| -> Result<()> {
        if current != source { Store::switch_sqlite_session_root(&current,source.into())?; }
        let local = if cloud.is_none() { Some(cloud_restore::lock_library(source)?) } else { None };
        install(Session { config,cloud,_local: local },window,cx)
    })();
    if let Err(error) = result {
        window.replace_root(cx,|_,_| Opening { message: format!("{message}\n{error}") });
    } else if let Some(Some(root)) = window.root::<Workspace>() {
        root.update(cx,|workspace,cx| workspace.show_toast_kind(crate::ui::ToastKind::Error,&message,cx));
    }
}
