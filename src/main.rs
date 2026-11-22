#![recursion_limit = "512"]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(not(all(target_os = "macos", feature = "cloudkit-restore")))]
pub(crate) const APP_ID: &str = "com.s1dashu.quietmd";
#[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
pub(crate) const APP_ID: &str = env!("FEI_BUNDLE_ID");

#[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
mod cloud_restore;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
mod cloud_sync;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
mod cloud_subscription;
#[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
#[path = "ipad_cloud.rs"]
mod cloud_restore;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
mod cloud_trial;

mod app;
mod mobile;
#[cfg(target_os = "ios")]
mod mobile_batch;
#[cfg(target_os = "ios")]
mod mobile_reorder;
mod gpui_compat;
#[cfg(target_os = "ios")]
mod ipad;
#[cfg(target_os = "ios")]
mod ipad_browser;
#[cfg(target_os = "ios")]
mod ipad_notifications;
#[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
mod ipad_session;
#[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
mod ipad_import;
#[cfg(unix)]
mod big_alloc;
mod board;
#[cfg(unix)]
#[global_allocator]
static ALLOC: big_alloc::Alloc = big_alloc::Alloc;
mod shared_store;
mod quick_note;
mod assets;
mod changelog;
mod guides;
mod clipboard;
#[cfg(feature = "devtools")]
mod devtools;
/// Without the `devtools` feature (release builds) nothing reads FDL_SCRIPT.
#[cfg(not(feature = "devtools"))]
mod devtools {
    pub static RENDER_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub fn scripted() -> bool { false }
    pub fn start(_: &mut gpui::Window, _: &mut gpui::App) {}
    pub fn take_pick() -> Option<Vec<std::path::PathBuf>> { None }
    pub fn file_drag() -> bool { false }
}
mod docinfo;
mod docmenu;
mod editor;
mod highlight;
mod history;
mod home;
mod html;
mod input;
mod logging;
mod modes;
mod onboarding;
#[cfg(target_os = "macos")]
mod native_header;
#[cfg(not(target_os = "macos"))]
#[path = "native_header_fallback.rs"]
mod native_header;
mod printing;
mod search;
mod sidebar;
mod local_files;
mod local_folders;
mod system_trash;
mod sidebar_drag;
mod sidebar_drop;
mod home_folder;
mod folders;
mod file_history;
mod settings;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
mod sharing;
mod table;
mod l10n;
mod themes;
mod fonts;
mod font_classes;
mod font_downloads;
mod shortcuts;
#[allow(dead_code, unused_imports)]
mod feishu_theme;
#[allow(dead_code, unused_imports)]
mod editor_theme;
#[allow(dead_code, unused_imports)]
mod editor_ui;
mod theme;
mod transfer;
mod trash;
mod tree;
mod ui;
mod update;
#[cfg(target_family = "wasm")]
mod web;

use fdl_core::{docxml, lang, markdown, model, palette, paste, store, time, whiteboard};

use gpui::*;

pub(crate) const DATATABLE: bool = cfg!(feature = "datatable") && !FEI_NOTES;

pub(crate) const FEI_NOTES: bool = cfg!(any(target_os = "ios", all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")));

pub(crate) fn app_name() -> &'static str {
    if APP_ID == "local.feinotes.upstream-preview" { return settings::tr("飞记 · 上游体验", "Fei Notes Upstream Preview") }
    if env!("FEI_UPSTREAM_PREVIEW") == "1" { return settings::tr("飞记 · 上游云体验", "Fei Notes Upstream Cloud Preview") }
    if FEI_NOTES { settings::tr("飞记", "Fei Notes") } else { "Quiet.md" }
}

pub(crate) fn app_window_title(quick: bool) -> String {
    if in_shared_note() { format!("{} · {}", app_name(), settings::tr("共享文档", "Shared Document")) }
    else if quick { format!("{} · {}", app_name(), settings::tr("快速文档", "Quick Document")) }
    else if cfg!(all(target_os = "macos", feature = "cloudkit-restore")) && !FEI_NOTES {
        format!("{} · {}", app_name(), settings::tr("iCloud 恢复测试", "iCloud Restore Test"))
    } else { app_name().into() }
}

pub(crate) fn in_shared_note() -> bool {
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    { cloud_sync::shared::active().is_some() }
    #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
    { false }
}

#[cfg(target_os = "macos")]
fn set_app_icon() {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    let Some(png) = crate::assets::get("branding/app-icon.png") else { return };
    unsafe {
        // A bundle with its own icon keeps it (the preview's has a dev ribbon);
        // only a bare binary (`cargo run`) borrows the app icon.
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"CFBundleIconFile".as_ptr()];
        let own: *mut Object = msg_send![bundle, objectForInfoDictionaryKey: key];
        if !own.is_null() { return }
        let data: *mut Object = msg_send![class!(NSData), dataWithBytes:png.as_ptr() length:png.len()];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData:data];
        if !image.is_null() {
            // AppKit draws the icon into a bitmap at the image's size in points
            // and keeps it: the 1024 px PNG reads as 1024 pt, 2048² px (32 MB)
            // on a Retina screen. The Dock shows at most 256 pt.
            let _: () = msg_send![image, setSize: core_graphics::geometry::CGSize::new(256., 256.)];
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setApplicationIconImage:image];
            let _: () = msg_send![image, release];
        }
    }
}

/// The main window's options: at `at`, else at the remembered spot on its own
/// display, clamped into the work area.
pub(crate) fn main_window_options(scripted: bool, at: Option<Bounds<Pixels>>, cx: &mut App) -> WindowOptions {
    // Scripted dev runs must not steal keyboard focus from whatever the
    // user is typing into.
    let (display_id, mut bounds) = match at { Some(at) => (None, at), None => quick_note::main_window_bounds(cx) };
    // A half-point origin (an odd leftover height) makes AppKit grow the
    // window by a point when it aligns the frame to pixels.
    bounds.origin = bounds.origin.map(|v| v.round());
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // The frame is measured from that display's own top-left corner, so the
        // window has to be opened on it: anywhere else that number means
        // somewhere on another screen (#253).
        display_id,
        window_background: crate::theme::window_background(),
        app_id: Some(crate::APP_ID.into()),
        window_decorations: if cfg!(target_os = "linux") { Some(WindowDecorations::Client) } else { None },
        titlebar: Some(TitlebarOptions {
            title: Some(app_window_title(false).into()),
            appears_transparent: cfg!(any(target_os = "macos", target_os = "windows")),
            traffic_light_position: Some(point(px(20.), px(app::HEADER_H / 2. - 7.))),
        }),
        window_min_size: Some(size(px(app::DOC_MIN_W), px(480.))),
        // AppKit does not drag from the transparent title bar: the app marks its
        // own drag areas (ui::TitleBarExt) and moves the window itself.
        app_owns_titlebar_drag: true,
        focus: !scripted,
        ..Default::default()
    }
}

/// The main window, at the remembered spot, or where first launch's window was.
pub(crate) fn open_main_window(scripted: bool, at: Option<Bounds<Pixels>>, cx: &mut App) {
    let options = main_window_options(scripted, at, cx);
    cx.open_window(options, |window, cx| {
        devtools::start(window, cx);
        cx.new(|cx| {
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            if let Some(room) = cloud_sync::shared::active() { return app::Workspace::new_document_window(Some(room.note.clone()), window, cx); }
            app::Workspace::new(window, cx)
        })
    })
    .expect("open window");
}

/// Everything that needs the library: run once it is chosen (at once for an
/// existing one, after first launch's pages for a new one). The main view
/// opens in a new window, or takes over `into` — first launch's own window,
/// so the pages give way to the notes without a second window.
#[cfg(not(target_os = "ios"))]
pub(crate) fn start(existed: bool, scripted: bool, into: Option<&mut Window>, incoming: local_files::Incoming, cx: &mut App) -> Option<Entity<app::Workspace>> {
    if !FEI_NOTES { changelog::startup(existed); }
    shortcuts::init(cx);
    match shared_store::SharedStore::open() {
        Ok(s) => cx.set_global(s),
        Err(err) => logging::fatal("无法打开文档库 / Could not open the library", &err),
    }
    if let Some(r) = cx.global::<shared_store::SharedStore>().with(|s| s.migrated.take()) { changelog::migrated(&r); }
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    if !cloud_restore::local_only() && let Err(err) = cloud_sync::start(cx) { logging::fatal("无法打开同步测试库 / Could not open the sync test library", &err); }
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    cloud_subscription::watch(cx);
    if !scripted && !in_shared_note() && !cfg!(target_family = "wasm") { quick_note::install(cx); }
    app::set_menus(cx);
    if !FEI_NOTES && !cfg!(all(target_os = "macos", feature = "cloudkit-restore")) { update::init(cx); }
    let took = into.map(|window| window.replace_root(cx, app::Workspace::new));
    if took.is_none() { open_main_window(scripted, None, cx); }
    // A synced library (or one used elsewhere) moves to real folders only once the user agrees.
    if !cfg!(all(target_os = "macos", feature = "cloudkit-restore")) && let store::MigrationState::NeedsConsent(r) = store::Store::migration_state(&store::Store::root_dir()) && settings::migrate_ask_due() {
        match &took {
            Some(ws) => ws.update(cx, |w, cx| { w.confirm = Some(app::Confirm::Migrate(r)); cx.notify(); }),
            None => if let Some(ws) = cx.windows().into_iter().find_map(|w| w.downcast::<app::Workspace>()) {
                let _ = ws.update(cx, |w, _, cx| { w.confirm = Some(app::Confirm::Migrate(r)); cx.notify(); });
            },
        }
    }
    if !in_shared_note() { incoming.deliver(cx); }
    took
}

pub(crate) fn init_app(cx: &mut App) {
    // The editor and fields hide the pointer themselves while typing
    // (`pointer_hidden`); GPUI would also hide it for shortcuts.
    cx.set_cursor_hide_mode(CursorHideMode::Never);
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    editor::emoji::load_font(cx);
    editor::math::load_fonts(cx);
    font_downloads::init(cx);
    board::fonts::load(cx);
    editor::image::init_pictures(cx);
    input::bind_keys(cx);
    editor::keymap::bind_keys(cx);
    editor::image::bind_keys(cx);
    board::bind_keys(cx);
    settings::load();
    settings::apply_appearance(&settings::prefs().appearance);
    settings::sync_dark(cx);
    themes::load(&settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
    settings::check_fonts(cx);
    cx.observe_global::<font_downloads::Changed>(settings::font_changed).detach();
}


#[cfg(target_family = "wasm")]
fn main() { web::main() }

#[cfg(not(any(target_os = "ios", target_family = "wasm")))]
fn main() {
    logging::init();
    #[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
    {
        #[cfg(feature = "cloudkit-sync")]
        let interactive = !std::env::args().skip(1).any(|arg| arg.starts_with("--cloud-") || arg == "--shared-note-room")
            && cloud_sync::test_root().is_ok_and(|root| if root.is_some() {
                env!("FEI_UPSTREAM_PREVIEW") == "1"
                    || devtools::scripted() && std::env::var_os("QUIET_SYNC_QA_STARTUP").is_some()
            } else { !devtools::scripted() });
        #[cfg(not(feature = "cloudkit-sync"))]
        let interactive = false;
        if !interactive { cloud_restore::startup(); }
        #[cfg(feature = "cloudkit-sync")]
        if interactive {
            match cloud_restore::cached_startup() {
                Ok(Some(reply)) => if let Err(error) = cloud_restore::install_startup(reply,false) { cloud_restore::startup_error(&error); },
                Ok(None) => { run_app(true); return; }
                Err(error) => cloud_restore::startup_error(&error),
            }
        }
    }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync", feature = "devtools"))]
    if std::env::args().nth(1).as_deref() == Some("--cloud-sync-qa") {
        let command = std::env::args().nth(2).unwrap_or_default();
        if let Err(error) = cloud_sync::qa::run(&command) { logging::fatal("Quiet Cloud QA", &error); }
        return;
    }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync", feature = "devtools"))]
    if std::env::args().nth(3).as_deref() == Some("--cloud-share-qa") {
        let command = std::env::args().nth(4).unwrap_or_default();
        if let Err(error) = cloud_sync::shared::live_room_smoke(&command) { logging::fatal("Quiet Cloud Sharing QA", &error); }
        return;
    }
    run_app(false);
}

#[cfg(not(any(target_os = "ios", target_family = "wasm")))]
fn run_app(_discover: bool) {
    let app = gpui_platform::application().with_assets(assets::Assets);
    // Markdown files opened from the Finder or the command line are edited in place.
    let incoming = local_files::Incoming::from_args();
    let urls = incoming.clone();
    if !in_shared_note() { app.on_open_urls(move |u| urls.push_urls(u)); }
    // macOS keeps running after ⌘W closes the last window (quick notes stay
    // available); clicking the Dock icon then brings the main window back.
    app.on_reopen(|cx| if cx.windows().is_empty() && cx.has_global::<shared_store::SharedStore>() && !onboarding::pending() { open_main_window(false, None, cx); });
    app.run(move |cx: &mut App| {
        app::bind_keys(cx);
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if _discover {
            settings::apply(Default::default());
            settings::sync_dark(cx);
            let scripted = devtools::scripted();
            let options = main_window_options(scripted,None,cx);
            let window = cx.open_window(options,|window,cx| {
                devtools::start(window,cx);
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    StartupHome { incoming: incoming.clone(), notes: Vec::new(), focus }
                })
            }).expect("open window");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() && !cx.has_global::<shared_store::SharedStore>() { cx.quit(); }
            }).detach();
            if !scripted { cx.activate(true); }
            cx.spawn(async move |cx| {
                if let Ok(notes) = cx.background_executor().spawn(async { cloud_restore::recent_local_notes() }).await {
                    let _ = window.update(cx,|home,_,cx| { home.notes = notes; cx.notify(); });
                }
            }).detach();
            cx.spawn(async move |cx| {
                let reply = cx.background_executor().spawn(async { cloud_restore::discover() }).await;
                // The local action can have already replaced this window's root.
                let _ = window.update(cx,|home,window,cx| home.finish(reply,window,cx));
            }).detach();
            return;
        }
        finish_launch(incoming,None,cx);
    });
}

#[cfg(not(any(target_os = "ios", target_family = "wasm")))]
fn finish_launch(incoming: local_files::Incoming, into: Option<&mut Window>, cx: &mut App) {
    let scripted = devtools::scripted();
    // No single-instance guard: copies may share a library (builds tested
    // side by side). `Store` rereads an index another copy rewrote before
    // writing its own, and `guides` seeds a new library only once.
    // The store asks the system whether a library is in iCloud through this.
    #[cfg(target_os = "macos")]
    store::set_ubiquity_probe(onboarding::ubiquitous);
    let root = store::Store::root_dir();
    logging::write(&format!("library {}", root.display()));
    // A library that was here before this launch can show 更新日志.
    let existed = guides::seeded(&root).unwrap_or(false);
    // A new library is set up after first launch's pages pick where it goes.
    let onboard = !cfg!(all(target_os = "macos", feature = "cloudkit-restore")) && onboarding::wanted(existed, scripted);
    if !scripted && !onboard && !in_shared_note() && let Err(err) = guides::initialize(&root) {
        logging::fatal(&format!("无法初始化文档库 / Could not set up the library at {}", root.display()), &err);
    }
    #[cfg(feature = "devtools")]
    if scripted && std::env::var_os("FDL_IN_WINDOW_PROMPTS").is_some() {
        cx.set_prompt_builder(gpui::fallback_prompt_renderer);
    }
    #[cfg(target_os = "macos")]
    set_app_icon();
    init_app(cx);
    let quick_enabled = !scripted && !in_shared_note();
    if onboard { onboarding::open(scripted, incoming, cx); } else { start(existed, scripted, into, incoming, cx); }
    cx.on_window_closed(move |cx, _| {
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cx.defer(cloud_sync::release_unused);
        // Closing first launch's window before it finished quits: there is no library yet.
        if (!quick_enabled || !cfg!(target_os = "macos") || onboarding::pending()) && cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    if !scripted {
        cx.activate(true);
    }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
struct StartupHome { incoming: local_files::Incoming, notes: Vec<(String,String)>, focus: FocusHandle }

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
impl StartupHome {
    fn finish(&self, reply: anyhow::Result<cloud_restore::Reply>, window: &mut Window, cx: &mut App) {
        if let Err(error) = reply.and_then(|reply| cloud_restore::install_startup(reply,false)) {
            cloud_restore::startup_error(&error);
        }
        finish_launch(self.incoming.clone(),Some(window),cx);
    }

    fn open_local(&self, id: Option<&str>, window: &mut Window, cx: &mut App) {
        self.finish(cloud_restore::choose_local(),window,cx);
        if let Some(Some(workspace)) = window.root::<app::Workspace>() {
            workspace.update(cx,|ws,cx| match id {
                Some(id) if ws.entries.iter().any(|e| e.id == id) => ws.open(id,window,cx),
                None => ws.new_doc(window,cx),
                _ => {}
            });
        }
    }
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
impl Render for StartupHome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut home = div().size_full().bg(theme::Theme::bg_body()).text_color(theme::Theme::text()).p_12()
            .track_focus(&self.focus)
            .on_action(cx.listener(|home, _: &app::NewDoc, window, cx| home.open_local(None,window,cx)))
            .on_action(|_: &app::CloseWindow, window, _| window.remove_window())
            .flex().flex_col().items_start().gap_4()
            .child(div().text_2xl().child(settings::tr("我的文档","My documents")))
            .child(ui::primary_button("startup-compose",settings::tr("新建本机文档","New On-Device Document"))
                .on_click(cx.listener(|home,_,window,cx| home.open_local(None,window,cx))));
        for (index,(id,title)) in self.notes.iter().enumerate() {
            let id = id.clone();
            home = home.child(div().id(("startup-note",index)).cursor_pointer().child(model::display_title(title))
                .on_click(cx.listener(move |home,_,window,cx| home.open_local(Some(&id),window,cx))));
        }
        home
    }
}
