//! 视频或文件 (`<figure><source/></figure>`): the file card, the preview
//! (the first page or frame from Quick Look's thumbnailer, the inline audio
//! player), its toolbar, and inserting picked or dropped files.

use fdl_core::vfs::{PathExt as _, KindExt as _};
use super::core::{DropPlace, Pos, Selection, TITLE};
use super::overlays::{Probe, at};
use super::render::{Env, R};
use super::view::{Editor, Menu};
use crate::store::image_path_in;
use crate::model::*;
use crate::store::Store;
use crate::editor_theme::{self as theme, Theme, craft};
use crate::editor_ui::{ROW_RADIUS, TipExt, icon};
use crate::design::toolbar::{TOOLBAR_H, toolbar_layout, w as tw};
use crate::design::{Button, ButtonGroupSeparator, ButtonSize, Toggle, Toolbar};
use super::image::{picture, resize_handles, resize_outline};
use gpui::{prelude::FluentBuilder, *};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// `.docx-width-mode .preview-wrap:not(audio):not(video) { max-width: 818px }`.
const PREVIEW_MAX_W: f32 = 818.;
/// New attachments open as 预览视图 under these sizes (documents, video).
const DOC_PREVIEW_MAX: u64 = 10 << 20;
const VIDEO_PREVIEW_MAX: u64 = 100 << 20;
const PREVIEW_MIN_W: f32 = 96.;
const AUDIO_MIN_W: f32 = 180.;
const AUDIO_PREVIEW_HEIGHT: f32 = 70.;
const VIDEO_PLAY_SIZE: f32 = 56.;
const PREVIEW_FOOTER_HEIGHT: f32 = 42.;
const PREVIEW_MAX_HEIGHT: f32 = 600.;
/// Cards in the page stay crisp: radius 5 with a 1pt hairline.
const RADIUS: f32 = 5.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pdf,
    Word,
    Excel,
    Ppt,
    Video,
    Audio,
    Image,
    Zip,
    Text,
    Code,
    Other,
}

fn ext_of(name: &str) -> String {
    Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase()
}

pub fn kind_of(name: &str) -> Kind {
    match ext_of(name).as_str() {
        "pdf" => Kind::Pdf,
        "doc" | "docx" | "rtf" | "pages" | "odt" | "wps" => Kind::Word,
        "xls" | "xlsx" | "csv" | "numbers" | "ods" | "et" => Kind::Excel,
        "ppt" | "pptx" | "key" | "odp" | "dps" => Kind::Ppt,
        "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "3gp" | "mpg" | "mpeg" => Kind::Video,
        "mp3" | "m4a" | "wav" | "aac" | "flac" | "aiff" | "aif" | "caf" | "ogg" | "opus" | "wma" | "amr" => Kind::Audio,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "tif" | "tiff" | "svg" => Kind::Image,
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" => Kind::Zip,
        "txt" | "md" | "markdown" | "log" => Kind::Text,
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "c" | "cc" | "cpp" | "h" | "hpp" | "json" | "html" | "css" | "sh" | "swift"
        | "kt" | "rb" | "php" | "xml" | "yaml" | "yml" | "toml" | "sql" => Kind::Code,
        _ => Kind::Other,
    }
}

/// What 预览视图 can show: a first page / frame, or the audio player for
/// the formats AVFoundation plays.
fn preview_of(name: &str) -> Option<Kind> {
    let k = kind_of(name);
    let ext = ext_of(name);
    match k {
        Kind::Zip | Kind::Other => None,
        Kind::Audio if !matches!(ext.as_str(), "mp3" | "m4a" | "wav" | "aac" | "flac" | "aiff" | "aif" | "caf") => None,
        Kind::Video if !matches!(ext.as_str(), "mp4" | "mov" | "m4v") => None,
        k => Some(k),
    }
}

fn preview_kind(file: &FileBlock) -> Option<Kind> {
    (file.view == FileView::Preview).then(|| preview_of(&file.name)).flatten()
}

#[derive(Clone, Copy)]
pub(super) struct PreviewFrame {
    pub(super) width: f32,
    pub(super) height: f32,
    min_width: f32,
    max_width: f32,
    height_per_width: f32,
    min_height: f32,
    default_height: f32,
    audio: bool,
}

impl PreviewFrame {
    fn visual(container: f32, kind: Kind, (tw, th): (f32, f32), layout: FilePreviewLayout) -> Self {
        let max_width = ((if kind == Kind::Video { container } else { container.min(PREVIEW_MAX_W) }) - 2.).max(0.);
        let min_width = max_width.min(PREVIEW_MIN_W);
        let ratio = th / tw.max(1.);
        let default_height = (max_width * ratio).clamp(if kind == Kind::Video { 160. } else { 120. }, PREVIEW_MAX_HEIGHT);
        // The default has a minimum height for very wide previews. Scale that
        // allowance down with the width as well, so the default remains reachable.
        let height_per_width = ratio.max(default_height / max_width.max(1.));
        let min_height = (min_width * ratio).max(VIDEO_PLAY_SIZE).min(default_height);
        let mut frame = Self { width: max_width, height: default_height,
            min_width, max_width, height_per_width, min_height, default_height, audio: false };
        frame.width = frame.clamp_width(layout.width.map_or(max_width, |w| w as f32));
        frame.height = frame.clamp_height(frame.width, layout.height.map_or(frame.max_height(frame.width), |h| h as f32));
        frame
    }

    fn audio(container: f32, layout: FilePreviewLayout) -> Self {
        let min_width = container.min(AUDIO_MIN_W);
        Self {
            width: layout.width.map_or(container, |w| (w as f32).clamp(min_width, container)),
            height: AUDIO_PREVIEW_HEIGHT, min_width, max_width: container, height_per_width: 0.,
            min_height: AUDIO_PREVIEW_HEIGHT, default_height: AUDIO_PREVIEW_HEIGHT, audio: true,
        }
    }

    fn outer_width(self) -> f32 { self.width + if self.audio { 0. } else { 2. } }

    fn clamp_width(self, w: f32) -> f32 { w.clamp(self.min_width, self.max_width) }

    fn max_height(self, w: f32) -> f32 { (w * self.height_per_width).clamp(self.min_height, self.default_height) }

    fn clamp_height(self, w: f32, h: f32) -> f32 { h.clamp(self.min_height, self.max_height(w)) }

    fn fitted(self, (tw, th): (f32, f32)) -> (f32, f32) {
        let scale = (self.width / tw.max(1.)).min(self.height / th.max(1.));
        (tw * scale, th * scale)
    }
}

fn preview_align(layout: FilePreviewLayout, block_align: Align) -> Align {
    layout.align.unwrap_or(if layout.width.is_some() { Align::Center } else { block_align })
}

pub(super) struct PreviewResize {
    pub(super) id: BlockId,
    pub(super) generation: u64,
    pub(super) cursor: CursorStyle,
    pub(super) frame: PreviewFrame,
    pub(super) draft: Option<FilePreviewLayout>,
    from: Point<Pixels>,
    axes: (i8, i8),
    original: FilePreviewLayout,
    align: Align,
}

impl PreviewResize {
    pub(super) fn drag(&mut self, p: Point<Pixels>, lock_aspect: bool) -> bool {
        let (x, y) = self.axes;
        let multiplier = if self.align == Align::Center { 2. } else { 1. };
        let dw = f32::from(p.x - self.from.x) * f32::from(x) * multiplier;
        let dh = f32::from(p.y - self.from.y) * f32::from(y);
        let (w, h) = (self.frame.width, self.frame.height);
        if !lock_aspect || self.frame.audio { return self.update(w + dw, h + dh); }

        // Scale the outside of the card, including the fixed filename bar.
        let fixed_height = PREVIEW_FOOTER_HEIGHT + 2.;
        let outer_w = self.frame.outer_width();
        let outer_h = h + fixed_height;
        let dx = dw / outer_w;
        let dy = dh / outer_h;
        let change = if dx.abs() >= dy.abs() { dx } else { dy };
        let min = ((self.frame.min_width + 2.) / outer_w).max((self.frame.min_height + fixed_height) / outer_h).min(1.);
        let max = ((self.frame.max_width + 2.) / outer_w).min((self.frame.default_height + fixed_height) / outer_h).max(1.);
        let dimensions = |scale: f32| (outer_w * scale - 2., outer_h * scale - fixed_height);
        let mut scale = (1. + change).clamp(min, max);
        let (target_w, target_h) = dimensions(scale);
        if target_h > self.frame.max_height(target_w) {
            // Find the furthest allowed size along the locked aspect ratio.
            let mut allowed = 1.;
            for _ in 0..16 {
                let middle = (allowed + scale) / 2.;
                let (w, h) = dimensions(middle);
                if h <= self.frame.max_height(w) { allowed = middle; } else { scale = middle; }
            }
            scale = allowed;
        }
        let (w, h) = dimensions(scale);
        self.update(w, h)
    }

    pub(super) fn update(&mut self, w: f32, h: f32) -> bool {
        let frame = self.frame;
        let width = frame.clamp_width(w.round());
        let height = frame.clamp_height(width, h.round());
        let width_changed = width.round() != frame.width.round();
        let height_changed = height.round() != frame.height.round();
        let next = (width_changed || height_changed).then(|| FilePreviewLayout {
            width: if width_changed { Some(width.round() as u32) } else { self.original.width },
            height: (!frame.audio).then(|| height.round() as u32),
            align: self.original.align.or(Some(self.align)),
        });
        if self.draft == next { return false; }
        self.draft = next;
        true
    }
}

pub fn previewable(name: &str) -> bool {
    preview_of(name).is_some()
}

/// The type's colour: the content palette (Tailwind v4 500 shades),
/// grey for plain text and unknown files.
fn kind_color(k: Kind) -> u32 {
    match k {
        Kind::Pdf => craft::R400,
        Kind::Word | Kind::Code => 0x2b7fff,
        Kind::Excel => 0x00bc7d,
        Kind::Ppt => craft::O400,
        Kind::Video => 0x8e51ff,
        Kind::Audio => 0x00b8db,
        Kind::Image => 0x00bba7,
        Kind::Zip => 0xfe9a00,
        Kind::Text | Kind::Other => 0x9ea4aa,
    }
}

fn glyph(k: Kind) -> &'static str {
    match k {
        Kind::Pdf => "editor-icons/file-g-pdf.svg",
        Kind::Word => "editor-icons/file-g-word.svg",
        Kind::Excel => "editor-icons/file-g-excel.svg",
        Kind::Ppt => "editor-icons/file-g-ppt.svg",
        Kind::Video => "editor-icons/file-g-video.svg",
        Kind::Audio => "editor-icons/file-g-audio.svg",
        Kind::Image => "editor-icons/file-g-image.svg",
        Kind::Zip => "editor-icons/file-g-zip.svg",
        Kind::Code => "editor-icons/file-g-code.svg",
        Kind::Text | Kind::Other => "editor-icons/file-g-text.svg",
    }
}

/// The file type icon: a page in the type's colour, its folded corner and
/// a white glyph.
pub fn type_icon(k: Kind, s: f32) -> Div {
    let layer = |path: &'static str, color: Hsla| svg().path(path).absolute().top_0().left_0().size(px(s)).text_color(color);
    div()
        .relative()
        .flex_none()
        .size(px(s))
        .child(layer("editor-icons/file-page.svg", theme::c(kind_color(k))))
        .child(layer("editor-icons/file-fold.svg", theme::rgbaf(255, 255, 255, 0.45)))
        .child(layer(glyph(k), gpui::white()))
}

/// The same icon painted into `b` (the inline chip).
pub fn paint_type_icon(k: Kind, b: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    for (path, color) in [("editor-icons/file-page.svg", theme::c(kind_color(k))), ("editor-icons/file-fold.svg", theme::rgbaf(255, 255, 255, 0.45)), (glyph(k), gpui::white())] {
        let _ = crate::gpui_compat::paint_svg(window, b, path.into(), TransformationMatrix::unit(), color, cx);
    }
}

/// `512 B`, `1.50KB`, `12.30MB`.
pub fn format_size(n: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.;
    let mut i = 0;
    while v >= 1024. && i < UNITS.len() - 1 {
        v /= 1024.;
        i += 1;
    }
    format!("{v:.2}{}", UNITS[i])
}

fn clock(t: f64) -> String {
    let s = t.max(0.).round() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// The file behind an attachment (cloud tokens have none).
pub fn local_path(root: &Path, f: &FileBlock) -> Option<PathBuf> {
    (f.src_kind != "token").then(|| image_path_in(root, &f.src)).flatten()
}

#[derive(Clone, PartialEq)]
enum Thumb {
    Pending,
    Ready(PathBuf),
    Failed,
}

pub(super) struct FileUi {
    root: PathBuf,
    thumbs: RefCell<HashMap<PathBuf, Thumb>>,
    queue: RefCell<Vec<PathBuf>>,
    sizes: RefCell<HashMap<PathBuf, Option<u64>>>,
    preview_columns: RefCell<HashMap<BlockId, f32>>,
    /// Box of every card / preview drawn this frame (window coordinates).
    pub(super) boxes: Rc<RefCell<HashMap<BlockId, Bounds<Pixels>>>>,
    /// Audio players by file, made when the player first shows.
    players: RefCell<HashMap<PathBuf, Option<Rc<player::Player>>>>,
    /// Progress bars of the audio players drawn this frame.
    bars: Rc<RefCell<HashMap<BlockId, Bounds<Pixels>>>>,
    muted: bool,
    ticking: bool,
}

impl FileUi {
    pub fn new() -> Self {
        FileUi {
            root: Store::root_dir(),
            thumbs: RefCell::default(),
            queue: RefCell::default(),
            sizes: RefCell::default(),
            preview_columns: RefCell::default(),
            boxes: Rc::default(),
            players: RefCell::default(),
            bars: Rc::default(),
            muted: false,
            ticking: false,
        }
    }

    pub(super) fn set_library_root(&mut self, root: PathBuf) { self.root = root; }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(super) fn invalidate_missing_sizes(&mut self) { self.sizes.get_mut().retain(|_, size| size.is_some()); }

    fn size(&self, p: &Path) -> Option<u64> {
        if let Some(s) = self.sizes.borrow().get(p) {
            return *s;
        }
        let s = fdl_core::vfs::metadata(p).ok().filter(|m| m.fs_is_file()).map(|m| m.len());
        self.sizes.borrow_mut().insert(p.to_path_buf(), s);
        s
    }

    fn thumb(&self, p: &Path, k: Kind) -> Thumb {
        if k == Kind::Image {
            return Thumb::Ready(p.to_path_buf());
        }
        if let Some(t) = self.thumbs.borrow().get(p) {
            return t.clone();
        }
        let t = match first_png(&thumb_dir(&self.root, p)) {
            Some(png) => Thumb::Ready(png),
            None => {
                self.queue.borrow_mut().push(p.to_path_buf());
                Thumb::Pending
            }
        };
        self.thumbs.borrow_mut().insert(p.to_path_buf(), t.clone());
        t
    }

    fn player(&self, p: &Path) -> Option<Rc<player::Player>> {
        if let Some(pl) = self.players.borrow().get(p) {
            return pl.clone();
        }
        let pl = player::Player::open(p).map(Rc::new);
        if let Some(pl) = &pl {
            pl.set_volume(if self.muted { 0. } else { 1. });
        }
        self.players.borrow_mut().insert(p.to_path_buf(), pl.clone());
        pl
    }

    fn any_playing(&self) -> bool {
        self.players.borrow().values().flatten().any(|p| p.playing())
    }
}

/// Where Quick Look's thumbnail of `p` goes (keyed by path, size and time).
fn thumb_dir(root: &Path, p: &Path) -> PathBuf {
    let mut h = rustc_hash::FxHasher::default();
    p.hash(&mut h);
    if let Ok(m) = fdl_core::vfs::metadata(p) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    Store::assets_in(root).join("thumbs").join(format!("{:016x}", h.finish()))
}

fn first_png(dir: &Path) -> Option<PathBuf> {
    fdl_core::vfs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "png"))
}

fn make_thumb(root: &Path, p: &Path) -> Option<PathBuf> {
    let dir = thumb_dir(root,p);
    fdl_core::vfs::create_dir_all(&dir).ok()?;
    if let Some(cached) = first_png(&dir) { return Some(cached); }
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" { fn fei_ios_thumbnail(path: *const std::ffi::c_char, destination: *const std::ffi::c_char) -> bool; }
        let output = dir.join("preview.png");
        let path = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).ok()?;
        let destination = std::ffi::CString::new(output.as_os_str().as_encoded_bytes()).ok()?;
        unsafe { fei_ios_thumbnail(path.as_ptr(),destination.as_ptr()) }.then_some(output)
    }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    { let output = dir.join("preview.png"); crate::cloud_sync::thumbnail(p,&output).then_some(output) }
    #[cfg(not(any(target_os = "ios", all(target_os = "macos", feature = "cloudkit-sync"))))]
    {
        let _ = std::process::Command::new("qlmanage")
            .args(["-t","-s","1200","-o"]).arg(&dir).arg(p)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        first_png(&dir)
    }
}

/// Opens the file in Quick Look (scripted runs only log it).
pub fn quick_look(p: &Path) {
    if crate::devtools::scripted() || cfg!(target_family = "wasm") {
        eprintln!("quicklook {}", p.display());
        return;
    }
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" { fn fei_ios_preview_file(path: *const std::ffi::c_char); }
        if let Ok(path) = std::ffi::CString::new(p.to_string_lossy().as_bytes()) { unsafe { fei_ios_preview_file(path.as_ptr()); } }
    }
    #[cfg(not(target_os = "ios"))]
    {
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    if crate::cloud_sync::preview(p) { return; }
    let mut cmd = std::process::Command::new("qlmanage");
    cmd.arg("-p").arg(p).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    std::thread::spawn(move || {
        let _ = cmd.status();
    });
    }
}

/// A new attachment for `p`, copied into `assets/files/<uuid>/<name>`.
fn file_block_from_path(root: &Path, p: &Path) -> Option<Block> {
    let name = p.file_name()?.to_string_lossy().to_string();
    let size = fdl_core::vfs::metadata(p).ok().filter(|m| m.is_file())?.len();
    let dir = Store::assets_in(root).join("files").join(uuid::Uuid::new_v4().to_string());
    fdl_core::vfs::create_dir_all(&dir).ok()?;
    let dest = dir.join(&name);
    fdl_core::vfs::copy(p, &dest).ok()?;
    let view = match preview_of(&name) {
        Some(Kind::Audio) => FileView::Preview,
        Some(Kind::Video) if size < VIDEO_PREVIEW_MAX => FileView::Preview,
        Some(Kind::Pdf | Kind::Word | Kind::Excel | Kind::Ppt) if size < DOC_PREVIEW_MAX => FileView::Preview,
        _ => FileView::Card,
    };
    let f = FileBlock { src_kind: "path".into(), src: crate::store::asset_source_in(root, &dest), name, view, preview: Default::default() };
    Some(Block::new(BlockKind::File(f), RichText::default()))
}

/// Opens the attachment right away (the card's 预览, the video's play).
fn open_listener(f: FileBlock, cx: &mut Context<Editor>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
        cx.stop_propagation();
        window.prevent_default();
        e.open_file(&f, cx);
    })
}

/// A floating toolbar's icon button.
/// A toolbar action (design::toolbar): a ghost `icon-sm` button.
fn tool_btn(id: impl Into<ElementId>) -> Button { Button::ghost(id).sized(ButtonSize::Sm) }

/// The audio player's buttons (in the page).
fn shell_btn(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .hover(|s| s.bg(Theme::fill_hover_strong()))
}

impl R<'_> {
    pub(super) fn file(&self, b: &Block, f: &FileBlock, env: Env, cx: &mut Context<Editor>) -> AnyElement {
        let ed = self.ed;
        let id = b.id;
        let container = if env.width > 0. { env.width } else { ed.content_width() };
        let active = !self.ghost && ed.active_file(self.focused) == Some(id);
        let selected = active
            || self.selected.contains(&id)
            || !self.ghost && !ed.core.sel.is_collapsed() && !ed.core.in_block_mode() && ed.core.atomic_selected(id, &self.units);
        let hovered = !self.ghost && !selected && ed.handle_block() == Some(id);
        // `hovered` covers the pointer over the block and its open handle
        // menu; only the menu gets the accent-tinted ring (as code blocks do).
        let menu_on = hovered && matches!(&ed.menu, Some(Menu::Block { block, .. }) | Some(Menu::Add { block, .. }) if *block == id);
        let path = local_path(&self.ed.library_root, f);
        let size = path.as_ref().and_then(|p| ed.file.size(p));
        let kind = kind_of(&f.name);
        let preview = size.and_then(|_| preview_kind(f));
        let layout = ed.file_resize_layout(id).unwrap_or(f.preview);
        let align = if preview.is_some() { preview_align(layout, b.align) } else { b.align };
        let st = theme::style();
        let attachment_bg = st.color("attachment.bg");
        let radius = st.num_override("attachment.radius").unwrap_or(RADIUS);
        let themed_card = st.num_override("attachment.radius").is_some();
        // The card rim: a hairline, a step darker under the pointer, the
        // accent once selected.
        let border = if selected {
            Theme::primary()
        } else if menu_on {
            Theme::code_hover_ring()
        } else if hovered {
            theme::c(craft::N400)
        } else {
            st.color("attachment.border").unwrap_or_else(Theme::button_border)
        };
        let ring = || div().absolute().top(px(-2.)).left(px(-2.)).right(px(-2.)).bottom(px(-2.)).border_1().border_color(Theme::code_hover_ring());
        let boxes = ed.file.boxes.clone();
        let recorder = (!self.ghost).then(|| {
            canvas(move |bounds, _, _| { boxes.borrow_mut().insert(id, bounds); }, |_, _, _, _| {})
                .absolute()
                .inset_0()
        });
        // A click on the selected attachment (any click when reading) opens it.
        let open_on_click = {
            let f = f.clone();
            cx.listener(move |e: &mut Editor, ev: &MouseDownEvent, window, cx| {
                if ev.click_count == 1 && !ev.modifiers.shift && (e.read_only || e.active_file(e.focus.is_focused(window)) == Some(id)) {
                    cx.stop_propagation();
                    window.prevent_default();
                    e.open_file(&f, cx);
                }
            })
        };
        if preview.is_some() && !self.ghost { ed.file.preview_columns.borrow_mut().insert(id, container); }
        let wrap = |card: Stateful<Div>, frame: PreviewFrame, cx: &mut Context<Editor>| {
            if self.ghost || !(active || hovered) { return card.into_any_element(); }
            let generation = ed.core.generation;
            let original = f.preview;
            let dots = (!cfg!(target_os = "ios") && !ed.read_only
                && (frame.max_width > frame.min_width || frame.max_height(frame.width) > frame.min_height)).then(|| {
                resize_handles(id, selected, frame.audio, cx, move |e, axes, cursor, from, window, cx| {
                    e.start_file_resize(PreviewResize { id, generation, cursor, frame, draft: None, from, axes, original, align }, window, cx);
                })
            });
            div().relative().flex_none().w(px(frame.outer_width()))
                .child(card)
                .child(resize_outline(radius, selected, border))
                .children(dots.into_iter().flatten())
                .into_any_element()
        };
        let body: AnyElement = match (preview, &path) {
            (Some(Kind::Audio), Some(p)) => {
                let frame = PreviewFrame::audio(container, layout);
                let pl = ed.file.player(p);
                let (t, d) = pl.as_ref().map_or((0., 0.), |pl| (pl.time(), pl.duration()));
                let playing = pl.as_ref().is_some_and(|pl| pl.playing());
                let frac = if d > 0. { (t / d).clamp(0., 1.) as f32 } else { 0. };
                let bars = ed.file.bars.clone();
                let bar_rec = (!self.ghost).then(|| {
                    canvas(move |bounds, _, _| { bars.borrow_mut().insert(id, bounds); }, |_, _, _, _| {})
                        .absolute()
                        .inset_0()
                });
                let ok = pl.is_some();
                let title = div()
                    .min_h(px(30.))
                    .mb(px(2.))
                    .flex()
                    .items_center()
                    .child(div().flex_1().min_w_0().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).text_ellipsis().whitespace_nowrap().overflow_hidden().child(f.name.clone()))
                    .child(div().ml(px(8.)).flex_none().text_size(px(12.)).text_color(Theme::text_caption()).child(if ok { format!("{} / {}", clock(t), clock(d)) } else { crate::settings::text("无法播放").into() }));
                let icon_c = if ok { Theme::icon() } else { Theme::text_disabled() };
                let play = shell_btn((if playing { "file-pause" } else { "file-play" }, id.0))
                    .tip_above(if playing { crate::settings::text("暂停") } else { crate::settings::text("播放") }, None)
                    .child(icon(if playing { "pause-filled" } else { "play-filled" }, 18., icon_c))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            e.toggle_audio(id, cx);
                        }),
                    );
                let muted = ed.file.muted;
                let volume = shell_btn((if muted { "file-unmute" } else { "file-mute" }, id.0))
                    .tip_above(if muted { crate::settings::text("取消静音") } else { crate::settings::text("静音") }, None)
                    .child(icon(if muted { "volume-x" } else { "volume-2" }, 18., icon_c))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            e.toggle_mute(cx);
                        }),
                    );
                let progress = div()
                    .id(("file-progress", id.0))
                    .relative()
                    .flex_1()
                    .h(px(12.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(Theme::line_border())
                            .child(div().absolute().left_0().top_0().bottom_0().w(relative(frac)).rounded(px(2.)).bg(Theme::primary())),
                    )
                    .children(bar_rec)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            e.seek_audio(id, ev.position.x, cx);
                        }),
                    );
                let card = div().id(("file-audio-preview", id.0)).relative()
                    .w(px(frame.outer_width()))
                    .flex()
                    .flex_col()
                    .child(title)
                    .child(
                        div()
                            .id(("file-audio", id.0))
                            .relative()
                            .px(px(if frame.width < AUDIO_MIN_W { 6. } else { 18. }))
                            .py(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(if frame.width < AUDIO_MIN_W { 5. } else { 14. }))
                            // Keep the player's hairline neutral; the selection
                            // ring surrounds both the title and the player.
                            .rounded(px(radius))
                            .bg(attachment_bg.unwrap_or_else(Theme::bg_overlay))
                            .border_1()
                            .border_color(st.color("attachment.border").unwrap_or_else(Theme::button_border))
                            .child(play)
                            .child(progress)
                            .child(volume),
                    )
                    .children(recorder);
                wrap(card, frame, cx)
            }
            (Some(k), Some(p)) => {
                let video = k == Kind::Video;
                let thumb = ed.file.thumb(p, k);
                let dims = match &thumb {
                    Thumb::Ready(t) => ed.img.dims(t),
                    _ => None,
                };
                let frame = dims.map(|dims| PreviewFrame::visual(container, k, dims, layout));
                let w = frame.map_or(if video { container } else { container.min(PREVIEW_MAX_W) }, PreviewFrame::outer_width);
                let iw = (w - 2.).max(0.);
                let content: AnyElement = match (&thumb, dims.zip(frame)) {
                    (Thumb::Ready(t), Some((dims, frame))) => {
                        let (pw, ph) = frame.fitted(dims);
                        let (left, top) = ((iw - pw) / 2., (frame.height - ph) / 2.);
                        let pic = picture(t, pw.max(ph)).object_fit(ObjectFit::Contain)
                            .absolute().left(px(left)).top(px(top)).w(px(pw)).h(px(ph));
                        let pic = pic.id(("file-thumb", id.0));
                        div()
                            .relative()
                            .w(px(iw))
                            .h(px(frame.height))
                            .overflow_hidden()
                            .bg(if video { gpui::black() } else { Theme::bg_body() })
                            .child(pic)
                            .when(video, |d| {
                                let l = open_listener(f.clone(), cx);
                                d.child(
                                    div().absolute().inset_0().flex().items_center().justify_center().child(
                                        div()
                                            .id(("file-play-video", id.0))
                                            .size(px(VIDEO_PLAY_SIZE))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(theme::rgbaf(0, 0, 0, 0.5))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::rgbaf(0, 0, 0, 0.65)))
                                            .tip_above(crate::settings::text("播放"), None)
                                            .child(icon("play-filled", 24., gpui::white()))
                                            .on_mouse_down(MouseButton::Left, l),
                                    ),
                                )
                            })
                            .into_any_element()
                    }
                    (t, _) => div()
                        .w(px(iw))
                        .h(px(220.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(attachment_bg.unwrap_or_else(Theme::bg_overlay))
                        .child(if *t == Thumb::Pending {
                            super::image::spinning("file-thumb-loading", 32., theme::c(craft::N400), 2.)
                        } else {
                            type_icon(k, 64.).into_any_element()
                        })
                        .into_any_element(),
                };
                let footer = div()
                    .h(px(PREVIEW_FOOTER_HEIGHT))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .bg(attachment_bg.unwrap_or_else(|| if video { Theme::bg_body() } else { Theme::bg_overlay() }))
                    .border_t_1()
                    .border_color(Theme::line_divider())
                    .child(type_icon(kind, 20.))
                    .child(div().ml(px(8.)).min_w_0().text_size(px(13.)).text_color(Theme::text()).text_ellipsis().whitespace_nowrap().overflow_hidden().child(f.name.clone()));
                let card = div()
                    .id(("file-preview", id.0))
                    .relative()
                    .w(px(w))
                    .rounded(px(radius))
                    .border_1()
                    .border_color(border)
                    .when(!themed_card, |d| d.shadow(theme::shadow_button()))
                    .cursor_pointer()
                    .when(menu_on, |d| d.child(ring().rounded(px(radius + 2.))))
                    .child(div().rounded(px((radius - 1.).max(0.))).overflow_hidden().child(content).child(footer))
                    .children(recorder)
                    .on_mouse_down(MouseButton::Left, open_on_click);
                match frame {
                    Some(frame) => wrap(card, frame, cx),
                    None => card.into_any_element(),
                }
            }
            _ => {
                let pending = size.is_none() && ed.shared_file_pending(f, cx);
                let missing = size.is_none() && !pending;
                let desc = match size {
                    Some(n) => format_size(n),
                    None if pending => crate::settings::tr("正在下载…", "Downloading…").into(),
                    None => crate::settings::text("文件不存在").into(),
                };
                let av = matches!(kind, Kind::Video | Kind::Audio);
                let l = open_listener(f.clone(), cx);
                let btn = div()
                    .id(("file-open", id.0))
                    .size(px(34.))
                    .mx(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius))
                    .when(!pending, |d| d.cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                        .tip_above(if av { crate::settings::text("播放") } else { crate::settings::text("预览") }, None))
                    .child(icon(if av { "circle-play" } else { "eye-open" }, 20., if size.is_none() { Theme::text_disabled() } else { Theme::icon() }))
                    .on_mouse_down(MouseButton::Left, l);
                // The file card: white, hairline, crisp radius 5, the button drop.
                div()
                    .id(("file-card", id.0))
                    .relative()
                    .flex()
                    .w(px(st.num("attachment.width")))
                    .max_w_full()
                    .min_w(px(200.))
                    .rounded(px(radius))
                    .border_1()
                    .border_color(border)
                    .bg(attachment_bg.unwrap_or_else(|| crate::theme::raised(Theme::bg_body())))
                    .when(!themed_card, |d| d.shadow(theme::shadow_button()))
                    .cursor_pointer()
                    .when(menu_on, |d| d.child(ring().rounded(px(radius + 2.))))
                    .child(div().flex_none().size(px(48.)).mt(px(8.)).mb(px(8.)).ml(px(8.)).mr(px(12.)).child(type_icon(kind, 48.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(64.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .child(div().max_h(px(42.)).text_size(px(15.)).line_height(px(21.)).text_color(Theme::text()).line_clamp(2).overflow_hidden().child(f.name.clone()))
                            .child(
                                div()
                                    .h(px(17.))
                                    .text_size(px(12.))
                                    .line_height(px(17.))
                                    .text_color(if missing { Theme::danger() } else { Theme::text_caption() })
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(desc),
                            ),
                    )
                    .child(div().flex_none().pr(px(4.)).mt(px(8.)).h(px(48.)).flex().items_center().child(btn))
                    .children(recorder)
                    .on_mouse_down(MouseButton::Left, open_on_click)
                    .into_any_element()
            }
        };
        div()
            .w_full()
            .flex()
            .font_family(theme::DOC_UI_FONT)
            .map(|d| match align {
                Align::Left => d.justify_start(),
                Align::Center => d.justify_center(),
                Align::Right => d.justify_end(),
            })
            .child(body)
            .into_any_element()
    }
}

impl Editor {
    pub(super) fn set_file_preview_layout(&mut self, id: BlockId, layout: FilePreviewLayout, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only || self.file_preview_frame(id).is_none() { return; }
        #[cfg(target_os = "ios")]
        if !crate::cloud_sync::shared::can_edit_in(&self.library_root, cx) { return; }
        if self.core.set_file_preview_layout(id, layout) { self.changed_preserving_viewport(window, cx); }
    }

    fn reset_file_frame(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(BlockKind::File(f)) = self.core.kind_of(id) else { return };
        self.set_file_preview_layout(id, FilePreviewLayout { align: f.preview.align, ..Default::default() }, window, cx);
    }

    /// The attachment showing as selected (toolbar, accent rim).
    pub(super) fn active_file(&self, focused: bool) -> Option<BlockId> {
        let h = self.core.sel.head.block;
        (focused
            && !self.core.in_block_mode()
            && self.core.cell_sel.is_none()
            && self.core.sel.is_collapsed()
            && matches!(self.core.kind_of(h), Some(BlockKind::File(_))))
        .then_some(h)
    }

    fn shared_file_pending(&self, f: &FileBlock, cx: &App) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { f.src_kind != "token" && crate::cloud_sync::shared_attachment_pending(&self.library_root, &f.src, cx) }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        { let _ = (f, cx); false }
    }

    pub(super) fn open_file(&mut self, f: &FileBlock, cx: &mut Context<Self>) {
        match local_path(&self.library_root, f).filter(|p| p.fs_is_file()) {
            Some(p) => quick_look(&p),
            None if self.shared_file_pending(f, cx) => {},
            None => self.toast(crate::settings::text("文件不存在"), cx),
        }
    }

    /// 下载: a copy where the save panel says (Downloads first).
    pub(super) fn download_file(&mut self, f: &FileBlock, cx: &mut Context<Self>) {
        let Some(src) = local_path(&self.library_root, f).filter(|p| p.fs_is_file()) else {
            if !self.shared_file_pending(f, cx) { self.toast(crate::settings::text("文件不存在"), cx); }
            return;
        };
        #[cfg(target_os = "ios")]
        {
            let name = Path::new(&f.name).file_name().map(|v|v.to_owned()).unwrap_or_else(||"Attachment".into());
            let session = Store::session_lease(&Store::root_dir());
            let task = cx.background_executor().spawn(async move {
                let _session = session;
                let dest = crate::ipad::export_folder()?.join(name);
                fdl_core::vfs::copy(src,&dest)?;
                anyhow::Ok(dest)
            });
            cx.spawn(async move |this,cx| {
                match task.await {
                    Ok(path)=>crate::ipad::present_export(&path,false),
                    Err(_)=>{ let _ = this.update(cx,|e,cx|e.toast(crate::settings::text("文件读取失败"),cx)); }
                }
            }).detach();
        }
        #[cfg(not(target_os = "ios"))]
        {
        let picked = crate::devtools::take_pick().and_then(|p| p.into_iter().next());
        let dir = dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default();
        let rx = picked.is_none().then(|| cx.prompt_for_new_path(&dir, Some(&f.name)));
        let bg = cx.background_executor().clone();
        cx.spawn(async move |_, _| {
            let dest = match rx {
                Some(rx) => rx.await.ok().and_then(Result::ok).flatten(),
                None => picked,
            };
            let Some(dest) = dest else { return };
            bg.spawn(async move {
                let _ = fdl_core::vfs::copy(&src, &dest);
            })
            .await;
        })
        .detach();
        }
    }

    // ---- audio

    fn audio_player(&self, id: BlockId) -> Option<Rc<player::Player>> {
        let Some(BlockKind::File(f)) = self.core.kind_of(id) else { return None };
        self.file.player(&local_path(&self.library_root, f)?)
    }

    fn toggle_audio(&mut self, id: BlockId, cx: &mut Context<Self>) {
        let Some(pl) = self.audio_player(id) else { return self.toast(crate::settings::text("无法播放该音频"), cx) };
        if pl.playing() {
            pl.pause();
        } else {
            // One at a time.
            for other in self.file.players.borrow().values().flatten() {
                if other.playing() {
                    other.pause();
                }
            }
            pl.play();
            self.tick_audio(cx);
        }
        cx.notify();
    }

    fn seek_audio(&mut self, id: BlockId, x: Pixels, cx: &mut Context<Self>) {
        let Some(pl) = self.audio_player(id) else { return };
        let Some(b) = self.file.bars.borrow().get(&id).copied() else { return };
        let k = ((x - b.left()) / b.size.width.max(px(1.))).clamp(0., 1.) as f64;
        pl.seek(k * pl.duration());
        cx.notify();
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.file.muted = !self.file.muted;
        let v = if self.file.muted { 0. } else { 1. };
        for pl in self.file.players.borrow().values().flatten() {
            pl.set_volume(v);
        }
        cx.notify();
    }

    /// Redraws the time and progress while something plays.
    fn tick_audio(&mut self, cx: &mut Context<Self>) {
        if self.file.ticking {
            return;
        }
        self.file.ticking = true;
        cx.spawn(async |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                let go = this
                    .update(cx, |e, cx| {
                        cx.notify();
                        let go = e.file.any_playing();
                        e.file.ticking = go;
                        go
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    // ---- thumbnails

    /// Makes queued Quick Look thumbnails off the main thread.
    pub(super) fn start_thumbs(&mut self, cx: &mut Context<Self>) {
        let queued: Vec<PathBuf> = self.file.queue.borrow_mut().drain(..).collect();
        for p in queued {
            let q = p.clone();
            let root = self.library_root.clone();
            let task = cx.background_executor().spawn(async move { make_thumb(&root, &q) });
            cx.spawn(async move |this, cx| {
                let t = task.await;
                let _ = this.update(cx, |e, cx| {
                    e.file.thumbs.borrow_mut().insert(p, t.map_or(Thumb::Failed, Thumb::Ready));
                    e.img.generation += 1;
                    cx.notify();
                });
            })
            .detach();
        }
    }

    // ---- toolbar

    /// 全屏 · 下载 | 文本视图 · 卡片视图 · 预览视图 above the selected attachment.
    pub(super) fn file_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.active_file(self.focus.is_focused(window))?;
        if self.resizing_image() || self.read_only || self.menu.is_some() || self.slash.is_some() || self.dragging || self.block_drag.is_some() {
            return None;
        }
        let rect = self.probed(Probe::File(id))?;
        let viewport = self.list.viewport_bounds();
        if !rect.intersects(&viewport) {
            return None;
        }
        let Some(BlockKind::File(f)) = self.core.kind_of(id) else { return None };
        let preview = preview_of(&f.name);
        let view = f.view;
        let act = |f: fn(&mut Editor, BlockId, &mut Window, &mut Context<Editor>)| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                f(e, id, window, cx);
            })
        };
        let sep = ButtonGroupSeparator::new;
        let mut bar = Toolbar::new("file-toolbar").on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        });
        let mut widths = vec![];
        if preview.is_some_and(|k| k != Kind::Audio) {
            let f = f.clone();
            bar = bar.child(
                tool_btn("ftb-full").editor_icon("maximize").tip_above(crate::settings::text("全屏"), None).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        e.open_file(&f, cx);
                    }),
                ),
            );
            widths.push(tw::BUTTON);
        }
        bar = bar
            .child(tool_btn("ftb-download").editor_icon("download").tip_above(crate::settings::text("下载"), None).on_mouse_down(
                MouseButton::Left,
                act(|e, id, _, cx| {
                    if let Some(BlockKind::File(f)) = e.core.kind_of(id).cloned() {
                        e.download_file(&f, cx);
                    }
                }),
            ))
            .child(sep())
            .child(tool_btn("ftb-text").editor_icon("view-text").tip_above(crate::settings::text("文本视图"), None).on_mouse_down(
                MouseButton::Left,
                act(|e, id, window, cx| {
                    e.core.file_to_inline(id);
                    e.changed(window, cx);
                }),
            ));
        widths.extend([tw::BUTTON, tw::SEPARATOR, tw::BUTTON]);
        let views: &[(&'static str, &'static str, &'static str, FileView)] = if preview.is_some() {
            &[("ftb-card", "view-card", "卡片视图", FileView::Card), ("ftb-preview", "view-preview", "预览视图", FileView::Preview)]
        } else {
            &[("ftb-card", "view-card", "卡片视图", FileView::Card)]
        };
        for &(key, ic, tip, v) in views {
            let on = view == v || preview.is_none();
            bar = bar.child(
                Toggle::new(key)
                    .editor_icon(ic)
                    .pressed(on)
                    .tip_above(tip, None)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            e.core.set_file_view(id, v);
                            e.changed_preserving_viewport(window, cx);
                        }),
                    ),
            );
            widths.push(tw::TOGGLE);
        }
        if self.file_preview_frame(id).is_some() {
            let layout = f.preview;
            let current = preview_align(layout, self.core.block(id)?.align);
            bar = bar.child(sep());
            widths.push(tw::SEPARATOR);
            for (key, icon, tip, align) in [
                ("ftb-left", "align-left", crate::settings::text("左对齐"), Align::Left),
                ("ftb-center", "align-center", crate::settings::text("居中对齐"), Align::Center),
                ("ftb-right", "align-right", crate::settings::text("右对齐"), Align::Right),
            ] {
                bar = bar.child(Toggle::new(key).editor_icon(icon).pressed(current == align).tip_above(tip, None)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        if align != current {
                            e.set_file_preview_layout(id, FilePreviewLayout { align: Some(align), ..layout }, window, cx);
                        }
                    })));
                widths.push(tw::TOGGLE);
            }
            if layout.width.is_some() || layout.height.is_some() {
                bar = bar.child(sep()).child(
                    tool_btn("ftb-reset-size").editor_icon("image-reset")
                        .tip_above(crate::settings::tr("恢复默认尺寸", "Restore default size"), None)
                        .on_mouse_down(MouseButton::Left, act(Editor::reset_file_frame)),
                );
                widths.extend([tw::SEPARATOR, tw::BUTTON]);
            }
        }
        let (_, w) = toolbar_layout(&widths);
        let x = (f32::from(rect.center().x) - w / 2.).clamp(f32::from(viewport.left()) + 8., (f32::from(viewport.right()) - w - 8.).max(f32::from(viewport.left()) + 8.));
        let above = f32::from(rect.top()) - 8. - TOOLBAR_H;
        let y = if above >= f32::from(viewport.top()) { above } else { f32::from(rect.bottom()) + 8. };
        Some(at(self.rel(point(px(x), px(y))), Anchor::TopLeft, bar))
    }

    /// The hover card of an inline attachment: 下载 | 文本视图 · 卡片视图 ·
    /// 预览视图, centred 4px above it.
    pub(super) fn file_card(&self, p: Pos, f: &FileBlock, cx: &mut Context<Self>) -> Option<AnyElement> {
        let r = self.probed(Probe::Button(p))?;
        let preview = previewable(&f.name);
        // 下载 | 文本视图 卡片视图 (预览视图): the views are toggles, this one pressed
        let n = if preview { 3 } else { 2 };
        let (_, w) = toolbar_layout(&[&[tw::BUTTON, tw::SEPARATOR][..], &vec![tw::TOGGLE; n]].concat());
        let file = f.clone();
        let sep = ButtonGroupSeparator::new();
        let mut bar = Toolbar::new("file-card-pop")
            .child(tool_btn("fcard-download").editor_icon("download").tip_above(crate::settings::text("下载"), None).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    e.hide_button_card(cx);
                    e.download_file(&file, cx);
                }),
            ))
            .child(sep)
            .child(Toggle::new("fcard-text").editor_icon("view-text").pressed(true).tip_above(crate::settings::text("文本视图"), None));
        let views: &[(&'static str, &'static str, &'static str, FileView)] = if preview {
            &[("fcard-card", "view-card", "卡片视图", FileView::Card), ("fcard-preview", "view-preview", "预览视图", FileView::Preview)]
        } else {
            &[("fcard-card", "view-card", "卡片视图", FileView::Card)]
        };
        for &(key, ic, tip, v) in views {
            bar = bar.child(Toggle::new(key).editor_icon(ic).tip_above(tip, None).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    if e.core.inline_to_file(p, v).is_some() {
                        e.hide_button_card(cx);
                        crate::gpui_compat::focus(window, &e.focus, cx);
                        e.changed(window, cx);
                    }
                }),
            ));
        }
        let el = bar
            .on_hover(cx.listener(|e, h: &bool, _, cx| {
                e.btn.card_over = *h;
                e.sync_button_card(cx);
            }))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
            });
        Some(at(self.rel(point(r.center().x - px(w / 2.), r.top() - px(4.))), Anchor::BottomLeft, el))
    }

    // ---- inserting

    /// 视频或文件: the picked files become attachments at the caret.
    pub(super) fn pick_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        let stamp = super::format_mobile::SelectionStamp::capture(self,"file-picker");
        #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
        let session = Store::session_lease(&Store::root_dir());
        let picked = crate::devtools::take_pick();
        let rx = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(crate::settings::text("插入视频或文件").into()), #[cfg(not(target_os = "ios"))] extensions: None }));
        cx.spawn_in(window, async move |this, cx| {
            #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
            let _session = session;
            let paths = match rx {
                Some(rx) => rx.await.ok().and_then(Result::ok).flatten(),
                None => picked,
            };
            let Some(paths) = paths.filter(|p| !p.is_empty()) else { return };
            let _ = this.update_in(cx, |e, window, cx| {
                #[cfg(target_os = "ios")]
                if !stamp.matches(e,stamp.serial,"file-picker") || e.read_only || !crate::cloud_sync::shared::can_edit_in(&e.library_root,cx) {
                    e.toast(crate::settings::tr("文档已更新，请重新插入。","The document changed. Please insert again."),cx); return;
                }
                e.import_files(paths,None,window,cx);
            });
        })
        .detach();
    }

    /// Copies `paths` into the library off the main thread, then inserts
    /// them at `target` (or the caret).
    pub(super) fn import_files(&mut self, paths: Vec<PathBuf>, target: Option<(BlockId, DropPlace)>, window: &mut Window, cx: &mut Context<Self>) {
        // A Markdown file keeps no attachments (dropped images still come in as images).
        if self.markdown { return; }
        #[cfg(target_os = "ios")]
        let stamp = super::format_mobile::SelectionStamp::capture(self,"file-import");
        #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
        let session = Store::session_lease(&Store::root_dir());
        let root = self.library_root.clone();
        let task = cx.background_executor().spawn(async move { paths.iter().map(|p| file_block_from_path(&root, p)).collect::<Vec<_>>() });
        cx.spawn_in(window, async move |this, cx| {
            #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
            let _session = session;
            let blocks = task.await;
            let _ = this.update_in(cx, |e, window, cx| {
                #[cfg(target_os = "ios")]
                if !stamp.matches(e,stamp.serial,"file-import") || e.read_only || !crate::cloud_sync::shared::can_edit_in(&e.library_root,cx) {
                    e.toast(crate::settings::tr("文档已更新，请重新插入。","The document changed. Please insert again."),cx); return;
                }
                let failed = blocks.iter().any(Option::is_none);
                let blocks: Vec<Block> = blocks.into_iter().flatten().collect();
                if failed {
                    e.toast(crate::settings::text("文件读取失败"), cx);
                }
                let Some(last) = blocks.last().map(|b| b.id) else { return };
                e.core.clear_block_sel();
                e.core.cell_sel = None;
                match target.filter(|(t, _)| e.core.block(*t).is_some()) {
                    Some((t, place)) => {
                        e.core.insert_blocks_at(blocks, t, place);
                        e.core.sel = Selection::caret(Pos::new(last, 0));
                    }
                    None => {
                        if e.core.sel.head.block == TITLE
                            && let Some(first) = e.core.doc.blocks.first().map(|b| b.id)
                        {
                            e.core.insert_blocks_at(blocks, first, DropPlace::Before);
                        } else {
                            e.core.insert_blocks(blocks);
                        }
                        let p = e.core.ensure_paragraph_after(last);
                        e.core.sel = Selection::caret(Pos::new(p, 0));
                    }
                }
                crate::gpui_compat::focus(window, &e.focus, cx);
                e.changed(window, cx);
            });
        })
        .detach();
    }
}

impl Editor {
    pub(super) fn file_preview_frame(&self, id: BlockId) -> Option<PreviewFrame> {
        let BlockKind::File(f) = self.core.kind_of(id)? else { return None };
        let kind = preview_kind(f)?;
        let path = local_path(&self.library_root, f)?;
        self.file.size(&path)?;
        let container = self.file.preview_columns.borrow().get(&id).copied().unwrap_or_else(|| self.content_width()).max(0.);
        if container <= 2. { return None; }
        let layout = self.file_resize_layout(id).unwrap_or(f.preview);
        if kind == Kind::Audio { return Some(PreviewFrame::audio(container, layout)); }
        let Thumb::Ready(thumb) = self.file.thumb(&path, kind) else { return None };
        Some(PreviewFrame::visual(container, kind, self.img.dims(&thumb)?, layout))
    }
}

#[cfg(target_os = "ios")]
impl Editor {
    pub(super) fn open_native_file_preview(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root, cx) || self.file_preview_frame(id).is_none() { return; }
        self.core.clear_block_sel();
        self.core.cell_sel = None;
        self.core.sel = Selection::caret(Pos::new(id, 0));
        crate::gpui_compat::focus(window, &self.focus, cx);
        self.menu = Some(Menu::NativeFilePreview { block: id });
        self.present_native_object(cx);
    }

    pub(super) fn native_file_preview_request(&self, id: BlockId) -> Option<serde_json::Value> {
        use super::object_mobile::field;
        use crate::settings::tr;
        use serde_json::json;
        let frame = self.file_preview_frame(id)?;
        let block = self.core.block(id)?;
        let BlockKind::File(f) = &block.kind else { return None };
        let alignment = super::object_mobile::alignment_field(preview_align(f.preview, block.align));
        let slider = |key, title, value: f32, min: f32, max: f32| {
            let mut slider = field(key, title, "slider", &value.to_string());
            slider["minimum"] = json!(min);
            slider["maximum"] = json!(max);
            slider
        };
        let mut fields = vec![alignment, slider("width", tr("宽度", "Width"), frame.width, frame.min_width, frame.max_width)];
        if !frame.audio {
            fields.push(slider("height", tr("高度", "Height"), frame.height, frame.min_height, frame.max_height(frame.width)));
        }
        if f.preview.width.is_some() || f.preview.height.is_some() {
            fields.push(field("reset", tr("恢复默认尺寸", "Restore default size"), "action", ""));
        }
        Some(json!({"kind":"file-preview","title":tr("附件预览尺寸", "Attachment preview size"),"live":true,"fields":fields}))
    }

    pub(super) fn mobile_file_preview_event(&mut self, id: BlockId, action: &str, edit: Option<(&str, &str)>, window: &mut Window, cx: &mut Context<Self>) {
        match (action, edit) {
            ("object-edit", Some((axis @ ("width" | "height"), value))) => {
                let Some(value) = value.parse::<f32>().ok().filter(|v| v.is_finite()) else { return };
                if !self.resizing_image() {
                    let Some(frame) = self.file_preview_frame(id) else { return };
                    let Some(block) = self.core.block(id) else { return };
                    let BlockKind::File(f) = &block.kind else { return };
                    let original = f.preview;
                    let align = preview_align(original, block.align);
                    self.start_file_resize(PreviewResize {
                        id, frame, original, align, generation: self.core.generation, draft: None,
                        from: point(px(0.), px(0.)), axes: (0, 0), cursor: CursorStyle::Arrow,
                    }, window, cx);
                    self.menu = Some(Menu::NativeFilePreview { block: id });
                }
                self.resize_file_axis(axis, value, cx);
            }
            ("object-edit", Some(("alignment", value))) => {
                self.end_resize(window, cx);
                let Some(block) = self.core.block(id) else { return };
                let BlockKind::File(f) = &block.kind else { return };
                if let Some(align) = super::object_mobile::alignment_value(value)
                    && align != preview_align(f.preview, block.align)
                {
                    self.set_file_preview_layout(id, FilePreviewLayout { align: Some(align), ..f.preview }, window, cx);
                }
            }
            ("object-action", Some(("resize-end", _))) => self.end_resize(window, cx),
            ("object-action", Some(("reset", _))) => {
                self.cancel_file_resize();
                self.reset_file_frame(id, window, cx);
            }
            ("object-submit" | "object-close", _) => {
                self.end_resize(window, cx);
                self.menu = None;
                cx.notify();
            }
            _ => {}
        }
    }
}


#[cfg(target_os = "macos")]
mod player {
    use objc::runtime::{BOOL, Object, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::path::Path;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {}

    pub struct Player(*mut Object);

    impl Player {
        pub fn open(path: &Path) -> Option<Self> {
            let s = path.to_str()?;
            unsafe {
                let ns: *mut Object = msg_send![class!(NSString), alloc];
                let ns: *mut Object = msg_send![ns, initWithBytes: s.as_ptr() as *const std::ffi::c_void length: s.len() encoding: 4usize];
                if ns.is_null() {
                    return None;
                }
                let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: ns];
                let _: () = msg_send![ns, release];
                if url.is_null() {
                    return None;
                }
                let p: *mut Object = msg_send![class!(AVAudioPlayer), alloc];
                let p: *mut Object = msg_send![p, initWithContentsOfURL: url error: std::ptr::null_mut::<*mut Object>()];
                if p.is_null() {
                    return None;
                }
                let _: BOOL = msg_send![p, prepareToPlay];
                Some(Player(p))
            }
        }

        pub fn play(&self) {
            unsafe {
                let _: BOOL = msg_send![self.0, play];
            }
        }

        pub fn pause(&self) {
            unsafe {
                let _: () = msg_send![self.0, pause];
            }
        }

        pub fn playing(&self) -> bool {
            unsafe {
                let b: BOOL = msg_send![self.0, isPlaying];
                b == YES
            }
        }

        pub fn time(&self) -> f64 {
            unsafe { msg_send![self.0, currentTime] }
        }

        pub fn seek(&self, t: f64) {
            unsafe {
                let _: () = msg_send![self.0, setCurrentTime: t];
            }
        }

        pub fn duration(&self) -> f64 {
            unsafe { msg_send![self.0, duration] }
        }

        pub fn set_volume(&self, v: f32) {
            unsafe {
                let _: () = msg_send![self.0, setVolume: v];
            }
        }
    }

    impl Drop for Player {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.0, stop];
                let _: () = msg_send![self.0, release];
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod player {
    use std::path::Path;

    pub struct Player;

    impl Player {
        pub fn open(_: &Path) -> Option<Self> {
            None
        }
        pub fn play(&self) {}
        pub fn pause(&self) {}
        pub fn playing(&self) -> bool {
            false
        }
        pub fn time(&self) -> f64 {
            0.
        }
        pub fn seek(&self, _: f64) {}
        pub fn duration(&self) -> f64 {
            0.
        }
        pub fn set_volume(&self, _: f32) {}
    }
}

#[cfg(test)]
mod tests {
    use super::{PreviewFrame, PreviewResize, Kind, clock, format_size, kind_of, previewable};
    use crate::model::{Align, BlockId, FilePreviewLayout};
    use gpui::{CursorStyle, point, px};

    #[test]
    fn preview_resize_bounds_follow_the_width_and_preserve_the_source_aspect() {
        let dims = (1600., 900.);
        let layout = FilePreviewLayout::default();
        let default = PreviewFrame::visual(642., Kind::Video, dims, layout);
        assert_eq!((default.width, default.height), (640., 360.));
        assert_eq!(default.min_width, 96.);
        let narrow = PreviewFrame::visual(642., Kind::Video, dims, FilePreviewLayout { width: Some(320), height: Some(500), ..layout });
        assert_eq!((narrow.width, narrow.height), (320., 180.));
        assert_eq!(narrow.max_height(240.), 135.);
        let short = PreviewFrame::visual(642., Kind::Video, dims, FilePreviewLayout { width: Some(240), height: Some(90), ..layout });
        assert_eq!(short.fitted(dims), (160., 90.));
        let portrait = PreviewFrame::visual(642., Kind::Video, (900., 1600.), layout);
        assert_eq!(portrait.max_height(500.), 600.);
        let wide_pdf = PreviewFrame::visual(1400., Kind::Pdf, (600., 900.), layout);
        assert_eq!(wide_pdf.outer_width(), 818.);
        let audio = PreviewFrame::audio(500., FilePreviewLayout { width: Some(80), height: Some(40), ..layout });
        assert_eq!((audio.width, audio.height, audio.max_height(180.)), (180., 70., 70.));
    }

    #[test]
    fn preview_corner_drags_can_resize_each_axis_or_lock_the_outer_aspect() {
        let original = FilePreviewLayout { width: Some(420), height: Some(220), align: Some(Align::Left) };
        let frame = PreviewFrame::visual(642., Kind::Video, (1600., 900.), original);
        let mut resize = PreviewResize { id: BlockId(2), generation: 0, cursor: CursorStyle::Arrow,
            frame, draft: None, from: point(px(0.), px(0.)), axes: (1, 1), original, align: Align::Left };
        assert!(resize.drag(point(px(-30.), px(-25.)), false));
        let draft = resize.draft.unwrap();
        assert_eq!((draft.width, draft.height), (Some(390), Some(195)));
        assert!(resize.drag(point(px(0.), px(-25.)), false));
        assert_eq!(resize.draft.unwrap().width, original.width);
        assert!(resize.drag(point(px(-50.), px(0.)), true));
        let draft = resize.draft.unwrap();
        let outer_aspect = (draft.width.unwrap() as f32 + 2.) / (draft.height.unwrap() as f32 + 44.);
        assert!((outer_aspect - 422. / 264.).abs() < 0.01);
    }

    #[test]
    fn sizes_and_kinds() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.50KB");
        assert_eq!(format_size(5 << 20), "5.00MB");
        assert_eq!(kind_of("报告.DOCX"), Kind::Word);
        assert_eq!(kind_of("a.tar.gz"), Kind::Zip);
        assert!(previewable("录音.m4a"));
        assert!(!previewable("clip.ogg"));
        assert!(!previewable("archive.zip"));
        assert_eq!(clock(75.4), "01:15");
    }
}
