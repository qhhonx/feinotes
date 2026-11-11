#![recursion_limit = "512"]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(not(all(target_os = "macos", feature = "cloudkit-restore")))]
pub(crate) const APP_ID: &str = "com.s1dashu.quietmd";
#[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
pub(crate) const APP_ID: &str = env!("FEI_BUNDLE_ID");

#[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
mod cloud_restore;
#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
mod cloud_sync;
#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
mod cloud_subscription;
#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
mod cloud_trial;

mod app;
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
mod home;
mod html;
mod input;
mod logging;
mod modes;
#[cfg(target_os = "macos")]
mod native_header;
#[cfg(not(target_os = "macos"))]
#[path = "native_header_fallback.rs"]
mod native_header;
mod printing;
mod search;
mod sidebar;
mod local_files;
mod sidebar_drag;
mod home_folder;
mod folders;
mod file_history;
mod settings;
mod themes;
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

use fdl_core::{docxml, lang, markdown, model, palette, paste, store, time};

use gpui::*;

pub(crate) const FEI_NOTES: bool = cfg!(all(target_os = "macos", feature = "cloudkit-sync"));

pub(crate) fn app_name() -> &'static str {
    if FEI_NOTES { settings::tr("飞记", "Fei Notes") } else { "Quiet.md" }
}

pub(crate) fn app_window_title(quick: bool) -> String {
    if in_shared_note() { format!("{} · {}", app_name(), settings::tr("共享笔记", "Shared Note")) }
    else if quick { format!("{} · {}", app_name(), settings::tr("快速笔记", "Quick Note")) }
    else if cfg!(all(target_os = "macos", feature = "cloudkit-restore")) && !FEI_NOTES {
        format!("{} · {}", app_name(), settings::tr("iCloud 恢复测试", "iCloud Restore Test"))
    } else { app_name().into() }
}

pub(crate) fn in_shared_note() -> bool {
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    { cloud_sync::shared::active().is_some() }
    #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
    { false }
}

#[cfg(target_os = "macos")]
fn set_app_icon() {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    let Ok(Some(png)) = assets::Assets.load("branding/app-icon.png") else { return; };
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), dataWithBytes:png.as_ptr() length:png.len()];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData:data];
        if !image.is_null() {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setApplicationIconImage:image];
            let _: () = msg_send![image, release];
        }
    }
}

/// The main window, centred at the remembered size.
fn open_main_window(scripted: bool, cx: &mut App) {
    // Scripted dev runs must not steal keyboard focus from whatever the
    // user is typing into.
    let bounds = Bounds::centered(None, quick_note::main_window_size(cx), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_background: crate::theme::window_background(),
            app_id: Some(crate::APP_ID.into()),
            window_decorations: if cfg!(target_os = "linux") { Some(WindowDecorations::Client) } else { None },
            titlebar: Some(TitlebarOptions {
                title: Some(app_window_title(false).into()),
                appears_transparent: cfg!(any(target_os = "macos", target_os = "windows")),
                traffic_light_position: Some(point(px(20.), px(app::HEADER_H / 2. - 7.))),
            }),
            window_min_size: Some(size(px(480.), px(480.))),
            focus: !scripted,
            ..Default::default()
        },
        |window, cx| {
            devtools::start(window, cx);
            cx.new(|cx| {
                #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                if let Some(room) = cloud_sync::shared::active() {
                    return app::Workspace::new_document_window(Some(room.note.clone()), window, cx);
                }
                app::Workspace::new(window, cx)
            })
        },
    )
    .expect("open window");
}

fn main() {
    logging::init();
    #[cfg(all(target_os = "macos", feature = "cloudkit-restore"))]
    cloud_restore::startup();
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
    let scripted = devtools::scripted();
    // No single-instance guard: copies may share a library (builds tested
    // side by side). `Store` rereads an index another copy rewrote before
    // writing its own, and `guides` seeds a new library only once.
    let root = store::Store::root_dir();
    logging::write(&format!("library {}", root.display()));
    // A library that was here before this launch can show 更新日志.
    let existed = guides::seeded(&root).unwrap_or(false);
    if !scripted && !in_shared_note() && let Err(err) = guides::initialize(&root) {
        logging::fatal(&format!("无法初始化文档库 / Could not set up the library at {}", root.display()), &err);
    }
    let app = Application::new().with_assets(assets::Assets);
    // Markdown files opened from the Finder or the command line are edited in place.
    let incoming = local_files::Incoming::from_args();
    let urls = incoming.clone();
    if !in_shared_note() { app.on_open_urls(move |u| urls.push_urls(u)); }
    // macOS keeps running after ⌘W closes the last window (quick notes stay
    // available); clicking the Dock icon then brings the main window back.
    app.on_reopen(|cx| if cx.windows().is_empty() { open_main_window(false, cx); });
    app.run(move |cx: &mut App| {
        #[cfg(feature = "devtools")]
        if scripted && std::env::var_os("FDL_IN_WINDOW_PROMPTS").is_some() {
            cx.set_prompt_builder(gpui::fallback_prompt_renderer);
        }
        #[cfg(target_os = "macos")]
        set_app_icon();
        editor::math::load_fonts(cx);
        editor::image::init_pictures(cx);
        input::bind_keys(cx);
        editor::keymap::bind_keys(cx);
        editor::image::bind_keys(cx);
        app::bind_keys(cx);
        settings::load();
        settings::apply_appearance(&settings::prefs().appearance);
        settings::sync_dark(cx);
        themes::load(&settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
        if !FEI_NOTES { changelog::startup(existed); }
        shortcuts::init(cx);
        match shared_store::SharedStore::open() {
            Ok(s) => cx.set_global(s),
            Err(err) => logging::fatal("无法打开文档库 / Could not open the library", &err),
        }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if !cloud_restore::local_only() && let Err(err) = cloud_sync::start(cx) {
            logging::fatal("无法打开同步测试库 / Could not open the sync test library", &err);
        }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        cloud_subscription::watch(cx);
        let quick_enabled = !scripted && !in_shared_note();
        if quick_enabled { quick_note::install(cx); }
        app::set_menus(cx);
        if !cfg!(all(target_os = "macos", feature = "cloudkit-restore")) { update::init(cx); }
        open_main_window(scripted, cx);
        if !in_shared_note() { incoming.deliver(cx); }
        cx.on_window_closed(move |cx| {
            if (!quick_enabled || !cfg!(target_os = "macos")) && cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if !scripted {
            cx.activate(true);
        }
    });
}
