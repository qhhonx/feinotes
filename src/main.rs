#![recursion_limit = "512"]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

pub(crate) const APP_ID: &str = "com.s1dashu.quietmd";

mod app;
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
/// Without the `devtools` feature (release builds) nothing reads FDL_SCRIPT;
/// only the built-in release self-test (`selftest.rs`) runs unattended.
#[cfg(not(feature = "devtools"))]
mod devtools {
    pub static RENDER_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub fn scripted() -> bool { crate::selftest::active() }
    pub fn start(_: &mut gpui::Window, _: &mut gpui::App) {}
    pub fn opened(_: &mut gpui::Window, _: &mut gpui::App) {}
    pub fn take_pick() -> Option<Vec<std::path::PathBuf>> { None }
    pub fn file_drag() -> bool { false }
    pub fn rows_begin(_: &gpui::Window) {}
    pub fn row_probe(_: impl Into<gpui::SharedString>) -> Option<gpui::AnyElement> { None }
    pub fn anchor_probe(_: impl Into<gpui::SharedString>) -> Option<gpui::AnyElement> { None }
}
mod design;
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
mod import_sources;
mod onboarding;
#[cfg(target_os = "macos")]
mod native_header;
#[cfg(not(target_os = "macos"))]
#[path = "native_header_fallback.rs"]
mod native_header;
mod mermaid;
mod on_top;
mod printing;
mod search;
mod siyuan;
#[cfg(not(target_family = "wasm"))]
mod selftest;
#[cfg(target_family = "wasm")]
mod selftest {
    pub fn active() -> bool { false }
    pub fn start(_: &mut gpui::Window, _: &mut gpui::App) {}
}
mod sidebar;
mod local_files;
mod loading;
mod local_folders;
mod sidebar_drag;
mod sidebar_drop;
mod home_folder;
mod folders;
mod file_history;
mod settings;
mod table;
mod l10n;
mod themes;
mod fonts;
mod font_classes;
mod font_downloads;
mod gallery;
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
mod entries;
mod tree_writes;
mod ui;
mod update;
#[cfg(target_family = "wasm")]
mod web;

use fdl_core::{docxml, lang, markdown, model, palette, paste, store, time, whiteboard};

use gpui::*;

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
            title: Some("Quiet.md".into()),
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
        selftest::start(window, cx);
        cx.new(|cx| app::Workspace::new(window, cx))
    })
    .expect("open window");
}

/// Everything that needs the library: run once it is chosen (at once for an
/// existing one, after first launch's pages for a new one). The main view
/// opens in a new window, or takes over `into` — first launch's own window,
/// so the pages give way to the notes without a second window.
pub(crate) fn start(existed: bool, scripted: bool, into: Option<&mut Window>, incoming: local_files::Incoming, cx: &mut App) -> Option<Entity<app::Workspace>> {
    changelog::startup(existed);
    shortcuts::init(cx);
    // No top-level folder is fatal: one that can't be opened shows as such in the sidebar.
    if !cx.has_global::<shared_store::SharedStore>() { cx.set_global(shared_store::SharedStore::open()); }
    let mut moved = vec![];
    cx.global::<shared_store::SharedStore>().each(|s| moved.extend(s.migrated.take()));
    for r in moved { changelog::migrated(&r); }
    // The top-level folder in front brings its own look (W21).
    settings::top_switched(cx);
    // The web build has no global shortcut to register.
    if !scripted && !cfg!(target_family = "wasm") { quick_note::install(cx); }
    app::set_menus(cx);
    update::init(cx);
    let took = into.map(|window| window.replace_root(cx, app::Workspace::new));
    if took.is_none() { open_main_window(scripted, None, cx); }
    // A synced library (or one used elsewhere) moves to real folders only once the user agrees.
    if let store::MigrationState::NeedsConsent(r) = store::Store::migration_state(&store::Store::active_dir()) && settings::migrate_ask_due() {
        match &took {
            Some(ws) => ws.update(cx, |w, cx| { w.confirm = Some(app::Confirm::Migrate(r)); cx.notify(); }),
            None => if let Some(ws) = cx.windows().into_iter().find_map(|w| w.downcast::<app::Workspace>()) {
                let _ = ws.update(cx, |w, _, cx| { w.confirm = Some(app::Confirm::Migrate(r)); cx.notify(); });
            },
        }
    }
    incoming.deliver(cx);
    took
}

/// Fonts, keys, preferences and theme: the same for the desktop and the web build.
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
    app::bind_keys(cx);
    settings::load();
    settings::apply_appearance(&settings::prefs().appearance);
    settings::sync_dark(cx);
    themes::load(&settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
    settings::check_fonts(cx);
    cx.observe_global::<font_downloads::Changed>(settings::font_changed).detach();
}

#[cfg(target_family = "wasm")]
fn main() { web::main() }

#[cfg(not(target_family = "wasm"))]
fn main() {
    // `--self-test`: runs the built-in release checks in launches of its own.
    if let Some(code) = selftest::command() { std::process::exit(code) }
    logging::init();
    // the design system's 组件库 panel alone: no library, no main window (src/gallery)
    if gallery::wanted() { return gallery::run(); }
    let scripted = devtools::scripted();
    // No single-instance guard: copies may share a library (builds tested
    // side by side). `Store` rereads an index another copy rewrote before
    // writing its own, and `guides` seeds a new library only once.
    // The store asks the system whether a library is in iCloud through this.
    #[cfg(target_os = "macos")]
    store::set_ubiquity_probe(onboarding::ubiquitous);
    let root = store::Store::legacy_dir();
    let fixed = std::env::var_os("QUIET_MD_HOME").is_some();
    logging::write(&format!("library {}", root.display()));
    // 主页: a folders.json (even an empty one: every folder removed) or the
    // library builds before it used means this computer has been here
    // before, and can show 更新日志.
    let existed = (!fixed && fdl_core::vfs::PathExt::fs_exists(fdl_core::home::Home::list_file().as_path())) || guides::seeded(&root).unwrap_or(false);
    // A first launch picks its folders on the first launch's pages.
    let onboard = onboarding::wanted(existed, scripted);
    // A test run's folder (`QUIET_MD_HOME`) is set up as before; the
    // self-test checks first launch's seeding, without its pages.
    if fixed && (!scripted || selftest::active()) && !onboard && let Err(err) = guides::initialize(&root) {
        logging::fatal(&format!("无法初始化文档库 / Could not set up the library at {}", root.display()), &err);
    }
    let app = gpui_platform::application().with_assets(assets::Assets);
    // Markdown files opened from the Finder or the command line are edited in place.
    let incoming = local_files::Incoming::from_args();
    let urls = incoming.clone();
    app.on_open_urls(move |u| urls.push_urls(u));
    // macOS keeps running after ⌘W closes the last window (quick notes stay
    // available); clicking the Dock icon then brings the main window back.
    app.on_reopen(|cx| if cx.windows().is_empty() && !onboarding::pending() { open_main_window(false, None, cx); });
    app.run(move |cx: &mut App| {
        #[cfg(target_os = "macos")]
        set_app_icon();
        init_app(cx);
        let quick_enabled = !scripted;
        if onboard { onboarding::open(scripted, incoming, cx); } else { start(existed, scripted, None, incoming, cx); }
        cx.on_window_closed(move |cx, _| {
            // Closing first launch's window before it finished quits: there is no library yet.
            if (!quick_enabled || !cfg!(target_os = "macos") || onboarding::pending()) && cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if !scripted {
            cx.activate(true);
        }
    });
}
