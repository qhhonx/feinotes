//! Image blocks: sizing, the corner resize handles, the crop tool, the image
//! toolbar, the lightbox viewer, dropped / pasted files, remote sources
//! (downloaded to a local cache) and the error toasts (image-spec,
//! crop-spec).

use fdl_core::vfs::PathExt as _;
use super::core::{DropPlace, EditKind, Pos, Selection, TITLE};
use super::overlays::{Probe, at};
use super::render::{Env, R};
use super::view::{Editor, EditorEvent};
use crate::store::image_path;
use crate::model::*;
use crate::store::Store;
use crate::editor_theme::{self as theme, Theme, craft};
use crate::editor_ui::{ROW_RADIUS, TipExt, ToastKind, icon};
use crate::design::toolbar::{TOOLBAR_H, toolbar_layout, w as tw};
use crate::design::{Button, ButtonGroupSeparator, ButtonSize, Toggle, Toolbar};
use gpui::{prelude::FluentBuilder, *};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

/// Smallest image width and crop box side (`ZM`).
pub(super) const MIN: f32 = 32.;
/// Size of an image whose pixels can't be measured.
const FALLBACK: (f32, f32) = (480., 270.);
const FORMATS: [&str; 6] = ["jpeg", "jpg", "png", "gif", "webp", "bmp"];
const MAX_BYTES: u64 = 20 * 1024 * 1024;
pub(super) const BAD_FORMAT: &str = "图片格式仅支持 jpeg、jpg、png、gif、webp、bmp";
const BAD_DIMS: &str = "识别图片宽高失败";
const TOO_BIG: &str = "上传图片大小不能超过 20 MB";
const FULL: [f32; 4] = [0., 0., 1., 1.];
const FADE: f32 = 0.2;
/// Media in the page stays crisp: image corners round at 5 (an image
/// shown cropped keeps square corners — GPUI can only round a whole image).
const RADIUS: f32 = 5.;

/// `f32::clamp` without the panic when `lo > hi`.
fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

/// `cubic-bezier(.34,.69,.1,1)`, roughly.
fn ease_out(k: f32) -> f32 {
    1. - (1. - k).powi(3)
}

actions!(image_viewer, [LbClose, LbPrev, LbNext, LbZoomIn, LbZoomOut, LbToggle, LbFit, LbRotate]);

/// The lightbox keys (viewer ~1195, ~4505).
pub fn bind_keys(cx: &mut App) {
    let c = Some("Lightbox");
    cx.bind_keys([
        KeyBinding::new("escape", LbClose, c),
        KeyBinding::new("left", LbPrev, c),
        KeyBinding::new("right", LbNext, c),
        KeyBinding::new("up", LbZoomIn, c),
        KeyBinding::new("cmd-=", LbZoomIn, c),
        KeyBinding::new("cmd-+", LbZoomIn, c),
        KeyBinding::new("down", LbZoomOut, c),
        KeyBinding::new("cmd--", LbZoomOut, c),
        KeyBinding::new("cmd-0", LbToggle, c),
        KeyBinding::new("cmd-1", LbToggle, c),
        KeyBinding::new("space", LbFit, c),
        KeyBinding::new("r", LbRotate, c),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
enum Remote {
    Queued,
    Pending,
    Failed,
}

/// Where an image's pixels come from this frame.
enum Src {
    Local(PathBuf),
    /// A local path that doesn't exist (`文件不存在`).
    Missing,
    /// A remote image still downloading.
    Pending,
    /// A remote image that couldn't be fetched, or a cloud token.
    Failed,
    /// A remote image not fetched: 设置 turned downloads off.
    Blocked,
}

struct Resize {
    id: BlockId,
    from_x: Pixels,
    start_w: f32,
    /// −1 for the left handles.
    dir: f32,
    /// A centred image grows on both sides.
    mul: f32,
    max: f32,
    aspect: f32,
    changed: bool,
    cursor: CursorStyle,
}

struct CropDrag {
    /// The edge dragged on each axis (−1 left/top, 1 right/bottom, 0 none).
    hx: i8,
    hy: i8,
    moving: bool,
    from: Point<Pixels>,
    /// x, y, w, h in displayed px at the start.
    start: [f32; 4],
    /// Displayed size of the whole image.
    size: (f32, f32),
    cursor: CursorStyle,
}

struct CropEdit {
    id: BlockId,
    /// Live box: left, top, right, bottom fractions of the whole image.
    rect: [f32; 4],
    drag: Option<CropDrag>,
}

struct Lightbox {
    ids: Vec<BlockId>,
    ix: usize,
    scale: f32,
    tx: f32,
    ty: f32,
    vp: (f32, f32),
    need_fit: bool,
    /// Quarter turns counter-clockwise.
    rot: u8,
    /// The last toggle went to 原始尺寸.
    orig: bool,
    opened: Instant,
    closing: Option<Instant>,
    chrome: bool,
    chrome_t: Instant,
    last_move: Instant,
    press: Option<(Point<Pixels>, f32, f32)>,
    moved: bool,
    /// The decoded, rotated (and cropped) pixels for (rot, image).
    rotated: Option<(u8, BlockId, Option<Arc<RenderImage>>)>,
}

pub struct ImageUi {
    dims: RefCell<HashMap<PathBuf, Option<(f32, f32)>>>,
    /// Box of every image drawn this frame (window coordinates).
    pub(super) boxes: Rc<RefCell<HashMap<BlockId, Bounds<Pixels>>>>,
    /// Width of the column each image was last laid out in.
    containers: RefCell<HashMap<BlockId, f32>>,
    pub(super) hover: Option<BlockId>,
    /// The image whose description is being typed.
    pub(super) caption_focus: Option<BlockId>,
    resize: Option<Resize>,
    crop: Option<CropEdit>,
    lightbox: Option<Lightbox>,
    lb_focus: FocusHandle,
    chrome_task: Option<Task<()>>,
    toast: Option<(ToastKind, SharedString, Instant)>,
    toast_task: Option<Task<()>>,
    remote: RefCell<HashMap<String, Remote>>,
    remote_dir: PathBuf,
    /// Bumped when remote images arrive or are retried (every row re-measures).
    pub(super) generation: u64,
    retried: HashMap<String, Instant>,
    /// Where dragged-in files would land, and the drop line.
    pub(super) file_drop: Option<(BlockId, DropPlace, Bounds<Pixels>)>,
}

type Decoded = Result<Arc<RenderImage>, ImageCacheError>;

/// One decode of `path` (a file may be decoded at several sizes). `ready`:
/// when the pixels arrived; `whole`: shown in a window that doesn't report
/// its frames, so its pixels are kept.
struct Slot { path: Option<Arc<Path>>, image: Option<Decoded>, used: Instant, windows: HashMap<WindowId, Instant>, generation: Arc<()>, waiters: Vec<EntityId>, ready: Option<Instant>, whole: bool, wanted: Arc<AtomicU64> }

/// Decoded document pictures (editor and Home cards). gpui's global asset
/// cache keeps every image it has decoded until the app quits; this one lets go
/// of pictures unused for 30 seconds, or sooner when the offscreen cache
/// exceeds 64 MiB, freeing their pixels and atlas textures. Pictures are
/// decoded at the size they are shown (`picture`), not at the file's, and
/// once on screen keep only their atlas texture (`RenderImage::hollow`).
pub struct DocImages(Rc<RefCell<HashMap<Resource, Slot>>>, u8);

thread_local! {
    static PICTURES: std::cell::OnceCell<Entity<DocImages>> = const { std::cell::OnceCell::new() };
    /// When each window last drew a frame.
    static FRAMES: RefCell<HashMap<WindowId, Instant>> = RefCell::default();
    /// The scale factor of the window being drawn.
    static SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(2.) };
    /// File and size behind each `picture` key.
    static KEYS: RefCell<HashMap<SharedUri, (Arc<Path>, u32)>> = RefCell::default();
}

/// Decodes at a time: half the cores, 2 to 4. Each holds a row of the file
/// and the shrunken picture (a JPEG its IDCT output), so the transient stays
/// small while a page of cards still fills about as fast as before.
static DECODERS: std::sync::LazyLock<async_lock::Semaphore> = std::sync::LazyLock::new(|| {
    async_lock::Semaphore::new(std::thread::available_parallelism().map_or(2, |n| n.get() / 2).clamp(2, 4))
});
/// Decodes waiting for a decoder, taken in the order they were asked for: the
/// order pictures paint in, top to bottom.
static QUEUE: std::sync::Mutex<Vec<Job>> = std::sync::Mutex::new(Vec::new());
static SEQ: AtomicU64 = AtomicU64::new(0);
/// When each window last drew a frame (`ms`), for the decoders.
static FRAME_MS: std::sync::Mutex<Vec<(WindowId, u64)>> = std::sync::Mutex::new(Vec::new());
static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
fn ms() -> u64 { START.elapsed().as_millis() as u64 }

type Work = Result<(Arc<Path>, u32), std::pin::Pin<Box<dyn std::future::Future<Output = Decoded> + Send>>>;
struct Job { seq: u64, work: Work, slot: std::sync::Weak<()>, wanted: Arc<AtomicU64>, window: WindowId, out: Arc<async_lock::OnceCell<Option<Decoded>>> }

/// Runs the earliest waiting decode, or skips it (None) when its slot is gone
/// or the last frame of its window no longer drew it (scrolled past).
async fn run_next() {
    let _permit = DECODERS.acquire().await;
    let job = { let mut q = QUEUE.lock().unwrap(); q.iter().enumerate().min_by_key(|(_, j)| j.seq).map(|(i, _)| i).map(|i| q.swap_remove(i)) };
    let Some(job) = job else { return };
    let frame = FRAME_MS.lock().unwrap().iter().find(|(w, _)| *w == job.window).map_or(0, |f| f.1);
    let skip = job.slot.strong_count() == 0 || job.wanted.load(Ordering::Relaxed) < frame;
    let d = if skip { None } else { Some(match job.work { Ok((p, c)) => decode(&p, c), Err(fut) => fut.await }) };
    if d.is_some() { DECODED.store(true, Ordering::Relaxed); }
    let _ = job.out.set(d).await;
}
/// A decode finished since the last sweep (its buffers are freed by now).
static DECODED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const UNUSED: Duration = Duration::from_secs(30);

pub fn init_pictures(cx: &mut App) {
    let e = cx.new(|cx: &mut Context<DocImages>| {
        cx.spawn(async |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if this.update(cx, |c, cx| c.sweep(cx)).is_err() { break }
        }).detach();
        DocImages(Rc::default(), 0)
    });
    PICTURES.with(|p| { let _ = p.set(e); });
}

/// Called once per frame from the window root.
pub fn frame_drawn(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    FRAMES.with(|f| f.borrow_mut().insert(id, Instant::now()));
    { let mut f = FRAME_MS.lock().unwrap(); let t = ms(); match f.iter_mut().find(|(w, _)| *w == id) { Some(e) => e.1 = t, None => f.push((id, t)) } }
    SCALE.with(|s| s.set(window.scale_factor()));
    // Hollow pictures this window couldn't draw (a new window, or its atlas
    // was rebuilt): decode them again.
    let missing = window.take_missing_images();
    if missing.is_empty() { return }
    let Some(e) = PICTURES.with(|p| p.get().cloned()) else { return };
    let map = e.read(cx).0.clone();
    let mut map = map.borrow_mut();
    let keys: Vec<Resource> = map.iter().filter(|(_, s)| matches!(&s.image, Some(Ok(im)) if im.is_hollow() && missing.contains(&im.id))).map(|(r, _)| r.clone()).collect();
    for r in keys {
        if let Some(Slot { image: Some(Ok(im)), .. }) = map.remove(&r) { cx.defer(move |cx| cx.drop_image(im, None)); }
    }
}

/// devtools: take every hollow picture out of this window's atlas and draw a
/// frame, as a rebuilt atlas (Windows device loss) would. Returns how many.
#[cfg(feature = "devtools")]
pub fn forget_uploads(window: &mut Window, cx: &mut App) -> usize {
    let Some(e) = PICTURES.with(|p| p.get().cloned()) else { return 0 };
    let map = e.read(cx).0.clone();
    let ims: Vec<_> = map.borrow().values().filter_map(|s| match &s.image { Some(Ok(im)) if im.is_hollow() => Some(im.clone()), _ => None }).collect();
    for im in &ims { let _ = window.drop_image(im.clone()); }
    window.refresh();
    ims.len()
}

/// Device pixels to decode for a long edge of `px`: rounded up to a step, so
/// resizing an image re-decodes it now and then, not every frame. 0 is the
/// file's own size.
fn cap(px: f32) -> u32 {
    if px.is_nan() || px <= 0. || px >= 8192. { return 0 }
    let step = if px <= 1024. { 128. } else { 512. };
    ((px / step).ceil() * step) as u32
}

/// A document picture, decoded for a long edge of `long` logical pixels in the
/// window being drawn (0: the file's full size, for the viewer). Layout still
/// sees the file's own pixel size.
pub fn picture(path: &Path, long: f32) -> Img {
    let cap = cap(long * SCALE.with(|s| s.get()));
    let uri = SharedUri::from(format!("docpic:{cap}:{}", path.display()));
    KEYS.with(|k| { k.borrow_mut().entry(uri.clone()).or_insert_with(|| (Arc::from(path), cap)); });
    let im = img(uri);
    match PICTURES.with(|p| p.get().cloned()) { Some(e) => im.image_cache(&e).load_visible_only(), None => im }
}

/// A board's image file (`board::render`), painted by hand: the pixels
/// straight from the picture cache, decoded for a long edge of `long`
/// logical pixels; None until they arrive (the view being painted is
/// notified then).
pub fn board_picture(path: &Path, long: f32, window: &mut Window, cx: &mut App) -> Option<Arc<RenderImage>> {
    let cap = cap(long * window.scale_factor());
    let uri = SharedUri::from(format!("docpic:{cap}:{}", path.display()));
    KEYS.with(|k| { k.borrow_mut().entry(uri.clone()).or_insert_with(|| (Arc::from(path), cap)); });
    let e = PICTURES.with(|p| p.get().cloned())?;
    e.update(cx, |c, cx| c.load(&Resource::Uri(uri), window, cx))?.ok()
}

/// Decodes `path` with its long edge at most `cap` pixels (0: as is). The
/// shrunken image keeps the file's size for layout through its scale factor.
fn decode(path: &Path, cap: u32) -> Decoded {
    let (im, w, _) = shrunk(path, cap)?;
    let mut rgba = im.into_rgba8();
    for px in rgba.as_chunks_mut::<4>().0 { px.swap(0, 2); }
    let scale = rgba.width() as f32 / w.max(1) as f32;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]).with_scale_factor(scale)))
}

/// `path`'s pixels as straight RGBA, the long edge at most `cap` (0: as
/// is), shrunk while decoding as `decode` does (board image sources).
pub fn decode_rgba(path: &Path, cap: u32) -> Option<image::RgbaImage> { shrunk(path, cap).ok().map(|(im, _, _)| im.into_rgba8()) }

/// `path` decoded with its long edge at most `cap` (0: as is), with the
/// file's own size.
fn shrunk(path: &Path, cap: u32) -> Result<(image::DynamicImage, u32, u32), ImageCacheError> {
    // A PNG streams from the file, never read whole.
    let (im, w, h) = match if cap > 0 { decode_png(path, cap) } else { None } {
        Some(p) => p,
        None => {
            let bytes = fdl_core::vfs::read(path)?;
            let format = image::guess_format(&bytes)?;
            let jpeg = if format == image::ImageFormat::Jpeg && cap > 0 {
                std::panic::catch_unwind(|| decode_jpeg(&bytes, cap)).ok().flatten()
            } else { None };
            match jpeg {
                Some(j) => j,
                None => { let im = image::load_from_memory_with_format(&bytes, format)?; let (w, h) = (im.width(), im.height()); (im, w, h) }
            }
        }
    };
    let long = w.max(h);
    // Shrink before adding alpha: a JPEG decodes to 3 bytes a pixel.
    let im = if cap > 0 && im.width().max(im.height()) > cap {
        let k = cap as f64 / long as f64;
        let small = im.thumbnail_exact(((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
        drop(im);
        small
    } else { im };
    Ok((im, w, h))
}

/// A PNG shrunk row by row as it decodes (box filter, alpha-weighted), so only
/// a row of the file and a row of sums are ever held, never the full image.
/// With the file's own size; None for what is left to `image` (interlaced,
/// or not larger than `cap`).
fn decode_png(path: &Path, cap: u32) -> Option<(image::DynamicImage, u32, u32)> {
    let mut d = png::Decoder::new(std::io::BufReader::new(fdl_core::vfs::File::open(path).ok()?));
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = d.read_info().ok()?;
    let info = r.info();
    let (w, h) = (info.width, info.height);
    if info.interlaced || w.max(h) <= cap { return None }
    let k = cap as f64 / w.max(h) as f64;
    let (tw, th) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    let n = match r.output_color_type() { (png::ColorType::Grayscale, _) => 1, (png::ColorType::GrayscaleAlpha, _) => 2, (png::ColorType::Rgb, _) => 3, (png::ColorType::Rgba, _) => 4, _ => return None };
    let col: Vec<u32> = (0..w).map(|x| (x as u64 * tw as u64 / w as u64) as u32).collect();
    // Per output pixel: alpha-weighted r, g, b, then alpha and pixel count.
    let mut sum = vec![[0u64; 5]; tw as usize];
    let mut out = vec![0u8; tw as usize * th as usize * 4];
    let flush = |sum: &mut [[u64; 5]], row: &mut [u8]| {
        for (s, px) in sum.iter_mut().zip(row.as_chunks_mut::<4>().0) {
            for c in 0..3 { if let Some(v) = (s[c] + s[3] / 2).checked_div(s[3]) { px[c] = v as u8; } }
            if let Some(a) = (s[3] + s[4] / 2).checked_div(s[4]) { px[3] = a as u8; }
            *s = [0; 5];
        }
    };
    let mut oy = 0;
    for y in 0..h {
        let ty = (y as u64 * th as u64 / h as u64) as u32;
        if ty != oy { flush(&mut sum, &mut out[(oy * tw * 4) as usize..((oy + 1) * tw * 4) as usize]); oy = ty; }
        let row = r.next_row().ok()??;
        for (x, p) in row.data().chunks_exact(n).enumerate() {
            let (v, a) = match n { 1 => ([p[0]; 3], 255), 2 => ([p[0]; 3], p[1]), 3 => ([p[0], p[1], p[2]], 255), _ => ([p[0], p[1], p[2]], p[3]) };
            let s = &mut sum[col[x] as usize];
            let a = a as u64;
            for c in 0..3 { s[c] += v[c] as u64 * a; }
            s[3] += a;
            s[4] += 1;
        }
    }
    flush(&mut sum, &mut out[(oy * tw * 4) as usize..((oy + 1) * tw * 4) as usize]);
    Some((image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(tw, th, out)?), w, h))
}

/// A JPEG shrunk while it decodes (the IDCT outputs 1/2, 1/4 or 1/8 of each
/// block), so a photo never sits in memory at full size. With the file's own
/// size; None for what is left to `image` (CMYK, 16-bit).
fn decode_jpeg(bytes: &[u8], cap: u32) -> Option<(image::DynamicImage, u32, u32)> {
    use jpeg_decoder::{Decoder, PixelFormat};
    let mut d = Decoder::new(bytes);
    d.read_info().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    if w.max(h) > cap {
        // The IDCT only halves; a step up to 15% under the cap is close enough
        // (a 4032 px photo at a 2048 cap decodes at 2016, not in full).
        let k = cap as f64 * 0.85 / w.max(h) as f64;
        d.scale(((w as f64 * k).ceil() as u16).max(1), ((h as f64 * k).ceil() as u16).max(1)).ok()?;
    }
    let pixels = d.decode().ok()?;
    let out = d.info()?;
    let (ow, oh) = (out.width as u32, out.height as u32);
    let im = match out.pixel_format {
        PixelFormat::RGB24 => image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(ow, oh, pixels)?),
        PixelFormat::L8 => image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(ow, oh, pixels)?),
        _ => return None,
    };
    Some((im, w, h))
}

impl DocImages {
    /// Protects pictures used by any live window; ages out the rest in LRU
    /// order. A decode at another size of a picture on screen goes at once.
    fn sweep(&mut self, cx: &mut App) {
        let now = Instant::now();
        let live = cx.windows();
        let mut hollowed = 0;
        let gone: Vec<Arc<RenderImage>> = FRAMES.with(|f| {
            let mut f = f.borrow_mut();
            f.retain(|id, _| live.iter().any(|w| w.window_id() == *id));
            let mut map = self.0.borrow_mut();
            let mut cold = Vec::new();
            let mut on_screen = std::collections::HashSet::new();
            let mut bytes = 0usize;
            for (r, s) in map.iter_mut() {
                s.windows.retain(|id, _| f.contains_key(id));
                let shown = s.windows.iter().any(|(id, used)| f.get(id).is_some_and(|t| used >= t));
                if shown {
                    if let (Some(p), Some(Ok(_))) = (&s.path, &s.image) { on_screen.insert(p.clone()); }
                    // Drawn by every window since the pixels arrived: they are
                    // in the atlases, which never read them again.
                    if let (Some(Ok(im)), Some(ready)) = (&s.image, s.ready)
                        && !s.whole && !s.windows.is_empty() && s.windows.values().all(|used| *used > ready)
                        && let Some(h) = im.hollow() {
                        hollowed += 1;
                        s.image = Some(Ok(Arc::new(h)));
                    }
                    continue
                }
                let size = match &s.image {
                    Some(Ok(im)) => (0..im.frame_count()).map(|i| im.as_bytes(i).map_or(0, |bytes| bytes.len())).sum(),
                    _ => 0,
                };
                bytes += size;
                cold.push((r.clone(), s.used, size, s.path.clone()));
            }
            cold.sort_by_key(|(_, used, _, _)| *used);
            let mut out = Vec::new();
            for (r, used, size, path) in cold {
                let superseded = path.is_some_and(|p| on_screen.contains(&p));
                if !superseded && now - used < UNUSED && bytes <= 64 * 1024 * 1024 { continue }
                bytes = bytes.saturating_sub(size);
                if let Some(Slot { image: Some(Ok(im)), .. }) = map.remove(&r) { out.push(im); }
            }
            out
        });
        if !gone.is_empty() || hollowed > 0 || DECODED.swap(false, std::sync::atomic::Ordering::Relaxed) { self.1 = 3; }
        for im in gone { cx.drop_image(im, None); }
        // Command buffers can release their last references after this sweep.
        if self.1 > 0 { self.1 -= 1; release_freed(); }
    }

    /// Forgets every decode of one file, so the next frame decodes it again.
    fn remove(path: &Path, cx: &mut App) {
        let Some(e) = PICTURES.with(|p| p.get().cloned()) else { return };
        let map = e.read(cx).0.clone();
        let mut map = map.borrow_mut();
        let keys: Vec<Resource> = map.iter().filter(|(_, s)| s.path.as_deref() == Some(path)).map(|(r, _)| r.clone()).collect();
        for r in keys {
            if let Some(Slot { image: Some(Ok(im)), .. }) = map.remove(&r) { cx.defer(move |cx| cx.drop_image(im, None)); }
        }
    }

    /// While a picture decodes at a new size, another size of it stands in.
    fn stand_in(map: &mut HashMap<Resource, Slot>, path: &Option<Arc<Path>>, window: Option<(WindowId, Instant)>) -> Option<Decoded> {
        let path = path.as_ref()?;
        let s = map.values_mut().filter(|s| s.path.as_ref() == Some(path) && matches!(s.image, Some(Ok(_))))
            .max_by_key(|s| s.image.as_ref().and_then(|i| i.as_ref().ok()).map_or(0, |i| i.size(0).width.0))?;
        if let Some((id, t)) = window { s.used = t; s.windows.insert(id, t); }
        s.image.clone()
    }
}

/// macOS keeps freed large blocks dirty for reuse, still counted in the
/// footprint; hands them back to the system.
fn release_freed() {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" { fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize; }
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
}

fn key_of(r: &Resource) -> Option<(Arc<Path>, u32)> {
    let Resource::Uri(u) = r else { return None };
    KEYS.with(|k| k.borrow().get(u).cloned())
}

impl ImageCache for DocImages {
    fn peek(&self, r: &Resource) -> Option<Decoded> {
        let mut map = self.0.borrow_mut();
        if let Some(Some(d)) = map.get(r).map(|s| s.image.clone()) { return Some(d) }
        let path = map.get(r).map_or_else(|| key_of(r).map(|k| k.0), |s| s.path.clone());
        Self::stand_in(&mut map, &path, None)
    }
    fn load(&mut self, r: &Resource, window: &mut Window, cx: &mut App) -> Option<Decoded> {
        let id = window.window_handle().window_id();
        let now = Instant::now();
        let untracked = FRAMES.with(|f| !f.borrow().contains_key(&id));
        if let Some(s) = self.0.borrow_mut().get_mut(r) {
            s.used = now;
            s.windows.insert(id, s.used);
            s.whole |= untracked;
            s.wanted.store(ms(), Ordering::Relaxed);
            if s.image.is_some() { return s.image.clone() }
            if !s.waiters.contains(&window.current_view()) { s.waiters.push(window.current_view()); }
        } else {
            let key = key_of(r);
            let generation = Arc::new(());
            let wanted = Arc::new(AtomicU64::new(if untracked { u64::MAX } else { ms() }));
            self.0.borrow_mut().insert(r.clone(), Slot { path: key.as_ref().map(|k| k.0.clone()), image: None, used: now, windows: HashMap::from([(id, now)]), generation: generation.clone(), waiters: vec![window.current_view()], ready: None, whole: untracked, wanted: wanted.clone() });
            let generation = Arc::downgrade(&generation);
            // Animated GIFs keep all their frames at full size (gpui's loader).
            let gif = key.as_ref().is_some_and(|(p, _)| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif")));
            let work: Work = match key {
                Some((p, c)) if !gif => Ok((p, c)),
                Some((p, _)) => Err(Box::pin(<ImgResourceLoader as Asset>::load(Resource::Path(p), cx))),
                None => Err(Box::pin(<ImgResourceLoader as Asset>::load(r.clone(), cx))),
            };
            let out = Arc::new(async_lock::OnceCell::new());
            QUEUE.lock().unwrap().push(Job { seq: SEQ.fetch_add(1, Ordering::Relaxed), work, slot: generation.clone(), wanted, window: id, out: out.clone() });
            cx.background_executor().spawn(run_next()).detach();
            let (map, r) = (self.0.clone(), r.clone());
            cx.spawn(async move |cx| {
                let d = out.wait().await.clone();
                let mut map = map.borrow_mut();
                // Still wanted (not removed for a retry meanwhile)?
                if let Some(s) = map.get_mut(&r) && generation.ptr_eq(&Arc::downgrade(&s.generation)) {
                    match d {
                        Some(d) => {
                            s.image = Some(d);
                            s.ready = Some(Instant::now());
                            cx.update(|cx| { for view in s.waiters.drain(..) { cx.notify(view); } });
                        }
                        // Skipped as off screen: asked for again when drawn. A view
                        // reused from the last frame may still show it (the frame
                        // that looked newer just didn't paint it), so its views
                        // draw again and ask once more if it is there.
                        None => {
                            let waiters = map.remove(&r).map(|s| s.waiters).unwrap_or_default();
                            cx.update(|cx| { for view in waiters { cx.notify(view); } });
                        }
                    }
                }
                drop(map);
                // Decoding leaves big freed buffers behind (the file, RGB rows).
                release_freed();
            }).detach();
        }
        let mut map = self.0.borrow_mut();
        let path = map.get(r).and_then(|s| s.path.clone());
        Self::stand_in(&mut map, &path, Some((id, now)))
    }
}

impl ImageUi {
    pub fn new(cx: &mut App) -> Self {
        ImageUi {
            dims: RefCell::default(),
            boxes: Rc::default(),
            containers: RefCell::default(),
            hover: None,
            caption_focus: None,
            resize: None,
            crop: None,
            lightbox: None,
            lb_focus: cx.focus_handle(),
            chrome_task: None,
            toast: None,
            toast_task: None,
            remote: RefCell::default(),
            remote_dir: Store::assets_dir().join("remote"),
            generation: 0,
            retried: HashMap::new(),
            file_drop: None,
        }
    }

    fn remote_path(&self, url: &str) -> PathBuf {
        self.remote_dir.join(remote_name(url))
    }

    fn resolve(&self, src: &str) -> Src {
        if src.starts_with("http://") || src.starts_with("https://") {
            if self.remote.borrow().get(src) == Some(&Remote::Failed) {
                return Src::Failed;
            }
            let p = self.remote_path(src);
            if self.dims.borrow().contains_key(&p) || p.fs_exists() {
                return Src::Local(p);
            }
            // 设置 › 网络与隐私: with downloads off, only an image the user
            // retried is fetched.
            if !crate::settings::prefs().remote_images && !self.retried.contains_key(src) {
                return Src::Blocked;
            }
            self.remote.borrow_mut().entry(src.to_string()).or_insert(Remote::Queued);
            return Src::Pending;
        }
        match image_path(src) {
            Some(p) if self.dims.borrow().contains_key(&p) || p.fs_exists() => Src::Local(p),
            // The web demo fetches pictures on first use.
            Some(p) if fdl_core::vfs::pending(&p) => Src::Pending,
            Some(_) => Src::Missing,
            None => Src::Failed,
        }
    }

    /// Pixel size from the file header (cached).
    pub(super) fn dims(&self, p: &Path) -> Option<(f32, f32)> {
        if let Some(d) = self.dims.borrow().get(p) {
            return *d;
        }
        let d = image::ImageReader::open(p)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.into_dimensions().ok())
            .map(|(w, h)| (w as f32, h as f32));
        self.dims.borrow_mut().insert(p.to_path_buf(), d);
        d
    }

    fn local(&self, src: &str) -> Option<PathBuf> {
        match self.resolve(src) {
            Src::Local(p) => Some(p),
            _ => None,
        }
    }
}

/// Laid-out size of an image.
#[derive(Clone, Copy)]
pub(super) struct Geo {
    /// The visible (cropped) part as shown.
    pub w: f32,
    pub h: f32,
    /// The whole image at the same zoom.
    pub fw: f32,
    pub fh: f32,
    pub crop: [f32; 4],
}

/// A crop that is in range, ordered and not the whole image.
fn valid_crop(c: Option<[f32; 4]>) -> Option<[f32; 4]> {
    c.filter(|c| c.iter().all(|v| (0. ..=1.).contains(v)) && c[0] < c[2] && c[1] < c[3] && *c != FULL)
}

/// `width`/`height` are the shown size of the visible part; without them the
/// image shows at its pixel size. Capped to the column (min 32 wide).
fn geometry(width: Option<f32>, height: Option<f32>, crop: Option<[f32; 4]>, natural: Option<(f32, f32)>, container: f32) -> Geo {
    let c = valid_crop(crop).unwrap_or(FULL);
    let (frw, frh) = (c[2] - c[0], c[3] - c[1]);
    let (w, h) = match (width, height, natural) {
        (Some(w), Some(h), _) if w > 0. && h > 0. => (w, h),
        (Some(w), _, Some((nw, nh))) if w > 0. && nw > 0. => (w, w * (nh * frh) / (nw * frw)),
        (_, _, Some((nw, nh))) if nw > 0. && nh > 0. => (nw * frw, nh * frh),
        _ => FALLBACK,
    };
    let dw = clampf(w, MIN, container.max(MIN));
    let dh = h * dw / w;
    Geo { w: dw, h: dh, fw: dw / frw, fh: dh / frh, crop: c }
}

/// The whole image while cropping: same zoom, capped to the column.
fn crop_display(g: &Geo, container: f32) -> (f32, f32) {
    let s = (container.max(MIN) / g.fw).min(1.);
    (g.fw * s, g.fh * s)
}

pub(super) fn spinning(id: &'static str, size: f32, color: Hsla, period: f32) -> AnyElement {
    svg()
        .path("editor-icons/spinner.svg")
        .size(px(size))
        .flex_none()
        .text_color(color)
        .with_animation(id, Animation::new(Duration::from_secs_f32(period)).repeat(), |s, t| {
            s.with_transformation(Transformation::rotate(percentage(t)))
        })
        .into_any_element()
}

/// `.loading`: the 50px grey spinner in the image's box.
fn loading_el() -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(spinning("img-loading", 50., theme::c(craft::N400), 2.))
        .into_any_element()
}

/// The error box: its content depends on the box size. A quiet fill in a
/// hairline, crisp corners, 13pt secondary text and a link-coloured 点击重试.
fn error_el(w: f32, h: f32, missing: bool, blocked: bool, border: Hsla, retry: Option<Rc<dyn Fn(&mut App)>>) -> AnyElement {
    let text = |s: &'static str| div().text_size(px(13.)).line_height(px(24.)).my(px(4.)).child(s);
    let link = |s: &'static str| {
        let r = retry.clone();
        div()
            .id("img-retry")
            .text_color(Theme::link())
            .cursor_pointer()
            .child(s)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                if let Some(r) = &r {
                    r(cx);
                }
            })
    };
    let body = if missing {
        (w > 58. && h > 58.).then(|| text(crate::settings::text("文件不存在")).into_any_element())
    } else if blocked && w > 220. && h > 58. {
        Some(text("").flex().child(crate::settings::tr("未自动下载网络图片，", "Web image not downloaded. ")).child(link(crate::settings::tr("点击加载", "Load"))).into_any_element())
    } else if w > 162. && h > 58. {
        Some(text("").flex().child(crate::settings::text("加载失败，")).child(link(crate::settings::text("点击重试"))).into_any_element())
    } else if w > 58. && h > 58. {
        Some(text("").child(link(crate::settings::text("点击重试"))).into_any_element())
    } else {
        None
    };
    div()
        .absolute()
        .inset_0()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(border)
        .bg(Theme::bg_overlay())
        .px(px(15.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .text_color(Theme::text_caption())
        .font_family(theme::DOC_UI_FONT)
        .when(w > 120. && h > 162., |d| d.child(icon("image-broken", 56., Theme::icon()).mb(px(8.))))
        .children(body)
        .into_any_element()
}

impl R<'_> {
    pub(super) fn image(&self, b: &Block, env: Env, cx: &mut Context<Editor>) -> AnyElement {
        let BlockKind::Image { src, width, height, caption, crop, .. } = &b.kind else { return div().into_any_element() };
        let ed = self.ed;
        let ui = &ed.img;
        let id = b.id;
        let container = if env.width > 0. { env.width } else { ed.content_width() };
        if !self.ghost {
            ui.containers.borrow_mut().insert(id, container);
        }
        let source = ui.resolve(src);
        let natural = match &source {
            Src::Local(p) => ui.dims(p),
            _ => None,
        };
        let g = geometry(*width, *height, *crop, natural, container);
        let cropping = !self.ghost && ui.crop.as_ref().is_some_and(|c| c.id == id);
        let (bw, bh) = if cropping { crop_display(&g, container) } else { (g.w, g.h) };
        let active = !self.ghost && ed.image_active(id, self.focused);
        let hovered = !self.ghost && ui.hover == Some(id);
        let show_caption = caption.as_ref().is_some_and(|c| !c.is_empty() || (active && !ed.read_only));
        let ok = matches!(source, Src::Local(_));
        // The accent marks only the selection; the pointer gets a
        // quiet darker hairline.
        let ring = if active {
            Theme::primary()
        } else if hovered {
            theme::ca(craft::N1000, 0.16)
        } else {
            Theme::line_border()
        };
        // Whole images round their corners; a crop (or the crop tool) shows square ones.
        let radius = if cropping || g.crop != FULL { 0. } else { RADIUS };

        let weak = cx.weak_entity();
        let src_s = src.clone();
        let retry: Rc<dyn Fn(&mut App)> = Rc::new(move |cx: &mut App| {
            let _ = weak.update(cx, |e, cx| e.retry_image(&src_s, cx));
        });
        let content = match source {
            Src::Local(p) => {
                let (iw, ih, ix, iy) = if g.crop != FULL && !cropping {
                    (g.fw, g.fh, -g.crop[0] * g.fw, -g.crop[1] * g.fh)
                } else {
                    (bw, bh, 0., 0.)
                };
                let r = retry.clone();
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .child(
                        picture(&p, iw.max(ih))
                            .absolute()
                            .left(px(ix))
                            .top(px(iy))
                            .w(px(iw))
                            .h(px(ih))
                            .rounded(px(radius))
                            .object_fit(ObjectFit::Fill)
                            .with_loading(loading_el)
                            .with_fallback(move || error_el(bw, bh, false, false, ring, Some(r.clone())))
                            .id(("img", id.0)),
                    )
                    .into_any_element()
            }
            Src::Pending => loading_el(),
            Src::Missing => error_el(bw, bh, true, false, ring, None),
            Src::Failed => error_el(bw, bh, false, false, ring, Some(retry)),
            Src::Blocked => error_el(bw, bh, false, true, ring, Some(retry)),
        };
        // `.img:after`: drawn inside the image — a 2pt accent ring when
        // selected, the quiet hairline under the pointer.
        let outline = (ok && !cropping && (active || hovered)).then(|| {
            let d = div().absolute().inset_0().rounded(px(radius));
            if active { d.border_2().border_color(Theme::primary()) } else { d.border_1().border_color(ring) }
        });
        // Resize handles: white dots with a soft drop, the accent rim once selected.
        let handles = (ok && !self.ghost && !cropping && !ed.read_only && (active || hovered)).then(|| {
            [(true, true), (false, true), (true, false), (false, false)].map(|(left, top)| {
                let cursor = if left == top { CursorStyle::ResizeUpLeftDownRight } else { CursorStyle::ResizeUpRightDownLeft };
                div()
                    .id(("img-handle", id.0 * 4 + left as u64 * 2 + top as u64))
                    .absolute()
                    .size(px(20.))
                    .map(|d| if left { d.left(px(-9.)) } else { d.right(px(-9.)) })
                    .map(|d| if top { d.top(px(-9.)) } else { d.bottom(px(-9.)) })
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(cursor)
                    .child(
                        div()
                            .size(px(10.))
                            .rounded_full()
                            .bg(gpui::white())
                            .border_1()
                            .border_color(if active { Theme::primary() } else { theme::ca(craft::N1000, 0.24) })
                            .shadow(vec![BoxShadow { color: Theme::shadow(), offset: point(px(0.), px(1.)), blur_radius: px(2.), spread_radius: px(0.), inset: false }]),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            e.start_resize(id, left, cursor, ev.position.x, window, cx);
                        }),
                    )
            })
        });
        let overlay = cropping.then(|| ui.crop.as_ref().map(|c| self.crop_overlay(c, bw, bh, cx))).flatten();
        let boxes = ui.boxes.clone();
        let recorder = (!self.ghost).then(|| {
            canvas(move |bounds, _, _| { boxes.borrow_mut().insert(id, bounds); }, |_, _, _, _| {})
                .absolute()
                .inset_0()
        });
        let image_box = div()
            .id(("image-box", id.0))
            .relative()
            .flex_none()
            .w(px(bw))
            .h(px(bh))
            .cursor(CursorStyle::PointingHand)
            .child(content)
            .children(outline)
            .when_some(handles, |d, hs| d.children(hs))
            .children(overlay)
            .children(recorder)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                    // A click on the selected image opens the viewer.
                    if ev.click_count == 1 && !ev.modifiers.shift && e.img.crop.is_none() && e.image_active(id, e.focus.is_focused(window)) {
                        cx.stop_propagation();
                        window.prevent_default();
                        e.open_lightbox(id, window, cx);
                    }
                }),
            );
        // `.caption-editor-area`: 6px below, as wide as the image; the block
        // keeps 4px below it (`.with-caption`). The caption: 13pt, light,
        // the caption grey, centred.
        let caption_el = show_caption.then(|| {
            let wrap = div().id(("image-caption", id.0)).mt(px(6.)).w_full().flex().font_family(theme::DOC_UI_FONT);
            match ed.caption_field(id).filter(|_| !self.ghost) {
                Some(f) => wrap
                    .child(f)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, _, cx| {
                            cx.stop_propagation();
                            e.core.clear_block_sel();
                            e.menu = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                None => wrap
                    .justify_center()
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(Theme::caption())
                    .child(caption.clone().unwrap_or_default())
                    .into_any_element(),
            }
        });
        div()
            .w_full()
            .flex()
            .map(|d| match b.align {
                Align::Left => d.justify_start(),
                Align::Center => d.justify_center(),
                Align::Right => d.justify_end(),
            })
            .when(show_caption, |d| d.pb(px(4.)))
            .child(div().flex().flex_col().w(px(bw)).child(image_box).children(caption_el))
            .into_any_element()
    }

    /// `.image-cropper`: the dimmed outside, the dashed box, its brackets and
    /// the 8 grab zones (crop-spec §2).
    fn crop_overlay(&self, c: &CropEdit, fw: f32, fh: f32, cx: &mut Context<Editor>) -> AnyElement {
        let id = c.id;
        let [l, t, r, b] = c.rect;
        let (x, y, w, h) = (l * fw, t * fh, (r - l) * fw, (b - t) * fh);
        let mask = |n: u64, x: f32, y: f32, w: f32, h: f32| {
            div()
                .id(("crop-mask", n))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w.max(0.)))
                .h(px(h.max(0.)))
                .bg(theme::rgbaf(0, 0, 0, 0.3))
                .cursor(CursorStyle::Arrow)
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                })
        };
        // Brackets: 6px bars, dark #2b2f36 inside a 1px white edge, 1px
        // outside the box.
        let k = |d: f32| ((d - 4.) / 2.).min(24.);
        let v = |d: f32| (d - 52.).min(24.);
        let (kw, kh) = (k(w), k(h));
        let mut bars: Vec<(f32, f32, f32, f32)> = vec![
            (-1., -1., kw, 6.),
            (-1., -1., 6., kh),
            (w + 1. - kw, -1., kw, 6.),
            (w + 1. - 6., -1., 6., kh),
            (-1., h + 1. - 6., kw, 6.),
            (-1., h + 1. - kh, 6., kh),
            (w + 1. - kw, h + 1. - 6., kw, 6.),
            (w + 1. - 6., h + 1. - kh, 6., kh),
        ];
        if w > 52. {
            let vw = v(w);
            bars.push(((w - vw) / 2., -1., vw, 6.));
            bars.push(((w - vw) / 2., h + 1. - 6., vw, 6.));
        }
        if h > 52. {
            let vh = v(h);
            bars.push((-1., (h - vh) / 2., 6., vh));
            bars.push((w + 1. - 6., (h - vh) / 2., 6., vh));
        }
        let bar = |(x, y, w, h): (f32, f32, f32, f32), inset: f32, color: Hsla| {
            div().absolute().left(px(x + inset)).top(px(y + inset)).w(px((w - 2. * inset).max(0.))).h(px((h - 2. * inset).max(0.))).bg(color)
        };
        let grab = |n: u64, hx: i8, hy: i8, moving: bool, cursor: CursorStyle| {
            div().id(("crop-grab", id.0 * 16 + n)).absolute().cursor(cursor).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    if moving && ev.click_count == 2 {
                        return e.save_crop(true, window, cx);
                    }
                    e.start_crop_drag(hx, hy, moving, cursor, ev.position, cx);
                }),
            )
        };
        let corner = |hx: i8, hy: i8| if hx == hy { CursorStyle::ResizeUpLeftDownRight } else { CursorStyle::ResizeUpRightDownLeft };
        let mut area = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .child(div().absolute().inset_0().border_1().border_dashed().border_color(theme::rgbaf(255, 255, 255, 0.8)))
            .child(grab(0, 0, 0, true, CursorStyle::OpenHand).inset_0());
        for bb in &bars {
            area = area.child(bar(*bb, 0., gpui::white()));
        }
        for bb in &bars {
            area = area.child(bar(*bb, 1., theme::c(craft::N800)));
        }
        let s = 24.;
        area = area
            .child(grab(1, -1, -1, false, corner(-1, -1)).left_0().top_0().size(px(s)))
            .child(grab(2, 1, -1, false, corner(1, -1)).right_0().top_0().size(px(s)))
            .child(grab(3, -1, 1, false, corner(-1, 1)).left_0().bottom_0().size(px(s)))
            .child(grab(4, 1, 1, false, corner(1, 1)).right_0().bottom_0().size(px(s)))
            .child(grab(5, -1, 0, false, CursorStyle::ResizeLeftRight).left_0().top(px(h / 2. - 12.)).size(px(s)))
            .child(grab(6, 1, 0, false, CursorStyle::ResizeLeftRight).right_0().top(px(h / 2. - 12.)).size(px(s)))
            .child(grab(7, 0, -1, false, CursorStyle::ResizeUpDown).top_0().left(px(w / 2. - 12.)).size(px(s)))
            .child(grab(8, 0, 1, false, CursorStyle::ResizeUpDown).bottom_0().left(px(w / 2. - 12.)).size(px(s)))
            .child(grab(9, 0, 0, true, CursorStyle::OpenHand).left(px(12.)).top(px(12.)).w(px((w - 24.).max(0.))).h(px((h - 24.).max(0.))));
        div()
            .absolute()
            .inset_0()
            .child(mask(id.0 * 4, 0., 0., fw, y))
            .child(mask(id.0 * 4 + 1, 0., y + h, fw, fh - y - h))
            .child(mask(id.0 * 4 + 2, 0., y, x, h))
            .child(mask(id.0 * 4 + 3, x + w, y, fw - x - w, h))
            .child(area)
            .into_any_element()
    }
}

impl Editor {
    /// The image showing as selected (toolbar, handles, accent ring).
    pub(super) fn active_image(&self, focused: bool) -> Option<BlockId> {
        if let Some(id) = self.img.caption_focus {
            return Some(id);
        }
        let h = self.core.sel.head.block;
        (focused
            && !self.core.in_block_mode()
            && self.core.cell_sel.is_none()
            && self.core.sel.is_collapsed()
            && matches!(self.core.kind_of(h), Some(BlockKind::Image { .. })))
        .then_some(h)
    }

    pub(super) fn image_active(&self, id: BlockId, focused: bool) -> bool {
        self.active_image(focused) == Some(id)
    }

    /// Geometry and column width of an image as last laid out.
    fn image_geo(&self, id: BlockId) -> Option<(Geo, f32)> {
        let BlockKind::Image { src, width, height, crop, .. } = self.core.kind_of(id)? else { return None };
        let container = self.img.containers.borrow().get(&id).copied().unwrap_or(self.content_width());
        let natural = self.img.local(src).and_then(|p| self.img.dims(&p));
        Some((geometry(*width, *height, *crop, natural, container), container))
    }

    /// Row keys that change with the image UI: the empty description shown
    /// while its image is active, and the uncropped image while cropping.
    pub(super) fn image_nonces(&self, focused: bool) -> Vec<(BlockId, u64)> {
        let mut out = vec![];
        if let Some(id) = self.active_image(focused)
            && matches!(self.core.kind_of(id), Some(BlockKind::Image { caption: Some(c), .. }) if c.is_empty())
        {
            out.push((id, 0x5bd1_e995_u64.wrapping_mul(id.0 | 1)));
        }
        if let Some(c) = &self.img.crop {
            out.push((c.id, 0x9e37_79b9_u64.wrapping_mul(c.id.0 | 1) ^ 0xc0ff_ee));
        }
        out
    }

    pub(super) fn image_hover(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let prev = self.img.hover;
        // The handles reach 9px outside the image.
        let hit = self
            .img
            .boxes
            .borrow()
            .iter()
            .find(|(id, b)| b.contains(&p) || (prev == Some(**id) && b.dilate(px(9.)).contains(&p)))
            .map(|(id, _)| *id);
        if hit != prev {
            self.img.hover = hit;
            cx.notify();
        }
    }

    fn retry_image(&mut self, src: &str, cx: &mut Context<Self>) {
        if self.img.retried.get(src).is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return;
        }
        self.img.retried.insert(src.to_string(), Instant::now());
        self.img.remote.borrow_mut().remove(src);
        let p = if src.starts_with("http://") || src.starts_with("https://") { Some(self.img.remote_path(src)) } else { image_path(src) };
        if let Some(p) = p {
            self.img.dims.borrow_mut().remove(&p);
            DocImages::remove(&p, cx);
        }
        self.img.generation += 1;
        cx.notify();
    }

    // ---- resize

    fn start_resize(&mut self, id: BlockId, left: bool, cursor: CursorStyle, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let Some((g, container)) = self.image_geo(id) else { return };
        let center = self.core.block(id).is_some_and(|b| b.align == Align::Center);
        window.focus(&self.focus, cx);
        self.menu = None;
        self.slash = None;
        self.core.clear_block_sel();
        self.core.cell_sel = None;
        self.core.sel = Selection::caret(Pos::new(id, 0));
        self.img.resize = Some(Resize {
            id,
            from_x: x,
            start_w: g.w,
            dir: if left { -1. } else { 1. },
            mul: if center { 2. } else { 1. },
            max: container.max(MIN),
            aspect: g.h / g.w,
            changed: false,
            cursor,
        });
        cx.notify();
    }

    pub(super) fn resizing_image(&self) -> bool {
        self.img.resize.is_some()
    }

    /// `width = start + round(dx)·dir·(2 if centred)`, 32 … column.
    pub(super) fn resize_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(r) = &self.img.resize else { return };
        let dx = f32::from(x - r.from_x).round();
        let w = clampf(r.start_w + dx * r.dir * r.mul, MIN, r.max);
        let (id, h) = (r.id, w * r.aspect);
        let same = matches!(self.core.kind_of(id), Some(BlockKind::Image { width: Some(cw), .. }) if (*cw - w).abs() < 0.01);
        if same || (!r.changed && (w - r.start_w).abs() < 0.01) {
            return;
        }
        if !r.changed {
            self.core.checkpoint(EditKind::Other);
        }
        if let Some(r) = &mut self.img.resize {
            r.changed = true;
        }
        if let Some(BlockKind::Image { width, height, .. }) = self.core.block_mut(id).map(|b| &mut b.kind) {
            *width = Some(w);
            *height = Some(h);
        }
        cx.notify();
    }

    pub(super) fn end_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.img.resize.take() {
            if r.changed {
                self.core.dirty = false;
                cx.emit(EditorEvent::Changed);
            }
            cx.notify();
        }
    }

    // ---- crop

    pub(super) fn cropping(&self) -> bool {
        self.img.crop.is_some()
    }

    fn start_crop(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.img.crop.as_ref().is_some_and(|c| c.id != id) {
            self.save_crop(false, window, cx);
        }
        let Some((g, _)) = self.image_geo(id) else { return };
        if g.w < MIN || g.h < MIN {
            return;
        }
        self.img.crop = Some(CropEdit { id, rect: g.crop, drag: None });
        cx.notify();
    }

    /// Writes the live box (4 decimals) as one undo step; the shown size
    /// keeps the zoom the whole image had while cropping.
    pub(super) fn save_crop(&mut self, reselect: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.img.crop.take() else { return };
        let round = |v: f32| (v * 10000.).round() / 10000.;
        let rect = c.rect.map(|v| clampf(round(v), 0., 1.));
        let new = valid_crop(Some(rect));
        if let Some((g, container)) = self.image_geo(c.id)
            && new.unwrap_or(FULL) != g.crop
        {
            let (fw, fh) = crop_display(&g, container);
            let r = new.unwrap_or(FULL);
            self.core.checkpoint(EditKind::Other);
            if let Some(BlockKind::Image { width, height, crop, .. }) = self.core.block_mut(c.id).map(|b| &mut b.kind) {
                *crop = new;
                *width = Some(fw * (r[2] - r[0]));
                *height = Some(fh * (r[3] - r[1]));
            }
            self.core.dirty = false;
            cx.emit(EditorEvent::Changed);
        }
        if reselect {
            window.focus(&self.focus, cx);
            self.core.clear_block_sel();
            self.core.cell_sel = None;
            self.core.sel = Selection::caret(Pos::new(c.id, 0));
        }
        cx.notify();
    }

    pub(super) fn cancel_crop(&mut self, cx: &mut Context<Self>) {
        if self.img.crop.take().is_some() {
            cx.notify();
        }
    }

    /// 重置图片: back to the whole image at the same zoom.
    fn reset_crop(&mut self, id: BlockId, cx: &mut Context<Self>) {
        self.img.crop = None;
        let Some(BlockKind::Image { crop, .. }) = self.core.kind_of(id) else { return };
        let Some(c) = valid_crop(*crop) else { return cx.notify() };
        self.core.checkpoint(EditKind::Other);
        if let Some(BlockKind::Image { width, height, crop, .. }) = self.core.block_mut(id).map(|b| &mut b.kind) {
            *crop = None;
            *width = width.map(|w| w / (c[2] - c[0]));
            *height = height.map(|h| h / (c[3] - c[1]));
        }
        self.core.dirty = false;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Saves the crop once its image stops being the selection; drops it
    /// when the image is gone.
    pub(super) fn sync_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &self.img.crop else { return };
        let id = c.id;
        if !matches!(self.core.kind_of(id), Some(BlockKind::Image { .. })) {
            self.img.crop = None;
            return;
        }
        if c.drag.is_some() {
            return;
        }
        let caret = !self.core.in_block_mode() && self.core.sel.is_collapsed() && self.core.sel.head.block == id;
        let owner = caret || self.core.block_sel_ids() == [id] || self.img.caption_focus == Some(id);
        if !owner {
            self.save_crop(false, window, cx);
        }
    }

    fn start_crop_drag(&mut self, hx: i8, hy: i8, moving: bool, cursor: CursorStyle, from: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(id) = self.img.crop.as_ref().map(|c| c.id) else { return };
        let Some((g, container)) = self.image_geo(id) else { return };
        let (fw, fh) = crop_display(&g, container);
        let Some(c) = &mut self.img.crop else { return };
        let [l, t, r, b] = c.rect;
        c.drag = Some(CropDrag {
            hx,
            hy,
            moving,
            from,
            start: [l * fw, t * fh, (r - l) * fw, (b - t) * fh],
            size: (fw, fh),
            cursor: if moving { CursorStyle::ClosedHand } else { cursor },
        });
        cx.notify();
    }

    pub(super) fn crop_dragging(&self) -> bool {
        self.img.crop.as_ref().is_some_and(|c| c.drag.is_some())
    }

    /// Moves or resizes the box from where the drag started (min 32, inside
    /// the image); Shift keeps its aspect.
    pub(super) fn crop_drag_to(&mut self, p: Point<Pixels>, shift: bool, cx: &mut Context<Self>) {
        let Some(c) = &mut self.img.crop else { return };
        let Some(d) = &c.drag else { return };
        let (dx, dy) = (f32::from(p.x - d.from.x), f32::from(p.y - d.from.y));
        let (fw, fh) = d.size;
        let [x, y, w, h] = d.start;
        let next = if d.moving {
            Some([clampf(x + dx, 0., fw - w), clampf(y + dy, 0., fh - h), w, h])
        } else {
            let axis = |e: i8, x: f32, w: f32, dx: f32, max: f32| match e {
                -1 => {
                    let nx = clampf(x + dx, 0., x + w - MIN);
                    (nx, x + w - nx)
                }
                1 => (x, clampf(w + dx, MIN, max - x)),
                _ => (x, w),
            };
            let (nx, nw) = axis(d.hx, x, w, dx, fw);
            let (ny, nh) = axis(d.hy, y, h, dy, fh);
            if !shift {
                Some([nx, ny, nw, nh])
            } else {
                let a = w / h;
                let r = match (d.hx, d.hy) {
                    (_, 0) => [nx, y + (h - nw / a) / 2., nw, nw / a],
                    (0, _) => [x + (w - nh * a) / 2., ny, nh * a, nh],
                    (_, hy) if w <= h => {
                        let nh = nw / a;
                        [nx, if hy < 0 { y + h - nh } else { y }, nw, nh]
                    }
                    (hx, _) => {
                        let nw = nh * a;
                        [if hx < 0 { x + w - nw } else { x }, ny, nw, nh]
                    }
                };
                let ok = r[2] >= MIN - 0.01 && r[3] >= MIN - 0.01 && r[0] >= -0.01 && r[1] >= -0.01 && r[0] + r[2] <= fw + 0.01 && r[1] + r[3] <= fh + 0.01;
                ok.then_some(r)
            }
        };
        let Some([nx, ny, nw, nh]) = next else { return };
        let rect = [nx / fw, ny / fh, (nx + nw) / fw, (ny + nh) / fh].map(|v| clampf(v, 0., 1.));
        if rect != c.rect {
            c.rect = rect;
            cx.notify();
        }
    }

    pub(super) fn end_crop_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.img.crop
            && c.drag.take().is_some()
        {
            cx.notify();
        }
    }

    /// The layer over the page while a handle or the crop box is dragged:
    /// keeps the cursor and the events with the drag.
    pub(super) fn image_drag_layer(&self) -> Option<AnyElement> {
        let cursor = match (&self.img.resize, &self.img.crop) {
            (Some(r), _) => r.cursor,
            (_, Some(CropEdit { drag: Some(d), .. })) => d.cursor,
            _ => return None,
        };
        Some(div().id("image-drag-layer").absolute().inset_0().occlude().cursor(cursor).into_any_element())
    }

    // ---- toolbar

    /// 裁剪 · 重置图片 | 添加描述 | 左 · 中 · 右 | 复制 above the selected image.
    pub(super) fn image_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let focused = self.focus.is_focused(window);
        let id = match &self.img.crop {
            Some(c) => c.id,
            None => self.active_image(focused)?,
        };
        if self.menu.is_some()
            || self.slash.is_some()
            || self.img.resize.is_some()
            || self.img.lightbox.is_some()
            || self.dragging
            || self.block_drag.is_some()
            || self.crop_dragging()
        {
            return None;
        }
        let rect = self.probed(Probe::Image(id))?;
        let viewport = self.list.viewport_bounds();
        if !rect.intersects(&viewport) {
            return None;
        }
        let b = self.core.block(id)?;
        let BlockKind::Image { src, caption, crop, .. } = &b.kind else { return None };
        let ok = self.img.local(src).is_some();
        let cropping = self.img.crop.as_ref().is_some_and(|c| c.id == id);
        let reset = match &self.img.crop {
            Some(c) if cropping => c.rect != FULL,
            _ => valid_crop(*crop).is_some(),
        };
        let small = !cropping && self.image_geo(id).is_some_and(|(g, _)| g.w.round() < MIN || g.h.round() < MIN);
        let has_caption = caption.as_ref().is_some_and(|c| !c.is_empty());
        let align = b.align;

        // A toolbar (design::toolbar): ghost icon-sm actions, toggles for
        // crop and the alignment.
        let btn = |id: &'static str, ic: &'static str| Button::ghost(id).sized(ButtonSize::Sm).editor_icon(ic);
        let sep = ButtonGroupSeparator::new;
        let act = |f: fn(&mut Editor, BlockId, &mut Window, &mut Context<Editor>)| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                f(e, id, window, cx);
            })
        };
        let mut bar = Toolbar::new("image-toolbar").on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        });
        let mut widths = vec![];
        if ok {
            let crop_btn = if small {
                Toggle::new("itb-crop")
                    .editor_icon("crop")
                    .disabled(true)
                    .tip_above(crate::settings::text("图片过小，请放大后重试"), None)
            } else {
                Toggle::new("itb-crop")
                    .editor_icon("crop")
                    .pressed(cropping)
                    .tip_above(if cropping { crate::settings::text("保存裁剪") } else { crate::settings::text("裁剪") }, None)
                    .on_mouse_down(
                        MouseButton::Left,
                        act(|e, id, window, cx| {
                            if e.img.crop.as_ref().is_some_and(|c| c.id == id) {
                                e.save_crop(true, window, cx);
                            } else {
                                e.start_crop(id, window, cx);
                            }
                        }),
                    )
            };
            bar = bar.child(crop_btn);
            widths.push(tw::TOGGLE);
            if reset {
                bar = bar.child(
                    btn("itb-reset", "image-reset")
                        .tip_above(crate::settings::text("重置图片"), None)
                        .on_mouse_down(MouseButton::Left, act(|e, id, _, cx| e.reset_crop(id, cx))),
                );
                widths.push(tw::BUTTON);
            }
            bar = bar.child(sep());
            widths.push(tw::SEPARATOR);
        }
        bar = bar
            .child(
                btn("itb-caption", "caption")
                    .tip_above(if has_caption { crate::settings::text("编辑描述") } else { crate::settings::text("添加描述") }, None)
                    .on_mouse_down(MouseButton::Left, act(|e, id, window, cx| e.edit_caption(id, window, cx))),
            )
            .child(sep());
        widths.extend([tw::BUTTON, tw::SEPARATOR]);
        for (key, ic, tip, a) in [
            ("itb-left", "align-left", crate::settings::text("左对齐"), Align::Left),
            ("itb-center", "align-center", crate::settings::text("居中对齐"), Align::Center),
            ("itb-right", "align-right", crate::settings::text("右对齐"), Align::Right),
        ] {
            let on = align == a;
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
                            e.core.set_align(&[id], a);
                            e.changed_preserving_viewport(window, cx);
                        }),
                    ),
            );
            widths.push(tw::TOGGLE);
        }
        if !cropping {
            bar = bar.child(sep()).child(
                btn("itb-copy", "copy")
                    .tip_above(crate::settings::text("复制"), None)
                    .on_mouse_down(MouseButton::Left, act(|e, id, _, cx| {
                        if e.copy_image(id, cx) { e.toast_kind(ToastKind::Success, crate::settings::text("已复制图片"), cx); }
                    })),
            );
            widths.extend([tw::SEPARATOR, tw::BUTTON]);
        }
        let (_, w) = toolbar_layout(&widths);
        let x = clampf(f32::from(rect.center().x) - w / 2., f32::from(viewport.left()) + 8., f32::from(viewport.right()) - w - 8.);
        let above = f32::from(rect.top()) - 8. - TOOLBAR_H;
        let y = if above >= f32::from(viewport.top()) { above } else { f32::from(rect.bottom()) + 8. };
        Some(at(self.rel(point(px(x), px(y))), Anchor::TopLeft, bar))
    }

    /// 添加描述 / 编辑描述: shows the description and puts the caret in it.
    fn edit_caption(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.img.crop.is_some() {
            self.save_crop(false, window, cx);
        }
        // An empty description is only shown while it's being typed (it's
        // dropped again on blur), so this isn't an undo step.
        if let Some(BlockKind::Image { caption, .. }) = self.core.block_mut(id).map(|b| &mut b.kind)
            && caption.is_none()
        {
            *caption = Some(String::new());
        }
        self.img.caption_focus = Some(id);
        if let Some(f) = self.caption_field(id) {
            window.focus(&f.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// The description field lost focus: an empty one goes away.
    pub(super) fn caption_blurred(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if self.img.caption_focus == Some(id) {
            self.img.caption_focus = None;
        }
        if let Some(BlockKind::Image { caption, .. }) = self.core.block_mut(id).map(|b| &mut b.kind)
            && caption.as_deref() == Some("")
        {
            *caption = None;
        }
        cx.notify();
    }

    // ---- lightbox

    fn open_lightbox(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let mut ids = vec![];
        self.core.doc.for_each_block(&mut |b: &Block| {
            if matches!(b.kind, BlockKind::Image { .. }) {
                ids.push(b.id);
            }
        });
        let ix = ids.iter().position(|i| *i == id).unwrap_or(0);
        let now = Instant::now();
        let vp = window.viewport_size();
        self.img.lightbox = Some(Lightbox {
            ids,
            ix,
            scale: 1.,
            tx: 0.,
            ty: 0.,
            vp: (f32::from(vp.width), f32::from(vp.height)),
            need_fit: true,
            rot: 0,
            orig: false,
            opened: now,
            closing: None,
            chrome: false,
            chrome_t: now - Duration::from_secs(1),
            last_move: now,
            press: None,
            moved: false,
            rotated: None,
        });
        window.focus(&self.img.lb_focus, cx);
        cx.notify();
    }

    fn close_lightbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        if lb.closing.is_none() {
            lb.closing = Some(Instant::now());
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// Path, crop and pixel size of the visible part of image `id`.
    fn lb_source(&self, id: BlockId) -> Option<(PathBuf, [f32; 4], f32, f32)> {
        let BlockKind::Image { src, crop, .. } = self.core.kind_of(id)? else { return None };
        let p = self.img.local(src)?;
        let c = valid_crop(*crop).unwrap_or(FULL);
        let (w, h) = match self.img.dims(&p) {
            Some((w, h)) => (w * (c[2] - c[0]), h * (c[3] - c[1])),
            None => self.image_geo(id).map(|(g, _)| (g.w, g.h))?,
        };
        Some((p, c, w, h))
    }

    /// Rotated pixel size of the current image.
    fn lb_size(&self) -> Option<(f32, f32)> {
        let lb = self.img.lightbox.as_ref()?;
        let (_, _, w, h) = self.lb_source(*lb.ids.get(lb.ix)?)?;
        Some(if lb.rot % 2 == 1 { (h, w) } else { (w, h) })
    }

    /// The initial fit (viewer `_` ~3809).
    fn lb_fit(&mut self) {
        let Some((w, h)) = self.lb_size() else { return };
        let Some(lb) = &mut self.img.lightbox else { return };
        let (vw, vh) = lb.vp;
        if h / w > 2.4 && h >= vh && w > 400. {
            lb.scale = (520f32).min(0.95 * vw) / w;
            lb.tx = 0.;
            lb.ty = (h * lb.scale - vh) / 2.;
        } else {
            let cap: f32 = if w < 400. || h < 400. {
                2.55
            } else if w < 800. || h < 800. {
                2.
            } else {
                1.
            };
            lb.scale = cap.min(0.95 * vw / w).min(0.9 * vh / h);
            lb.tx = 0.;
            lb.ty = 0.;
        }
        lb.need_fit = false;
    }

    /// Keeps the image edges outside the viewport; an axis that fits is centred.
    fn lb_clamp(&mut self) {
        let Some((w, h)) = self.lb_size() else { return };
        let Some(lb) = &mut self.img.lightbox else { return };
        let fix = |t: f32, size: f32, vp: f32| if size <= vp { 0. } else { clampf(t, -(size - vp) / 2., (size - vp) / 2.) };
        lb.tx = fix(lb.tx, w * lb.scale, lb.vp.0);
        lb.ty = fix(lb.ty, h * lb.scale, lb.vp.1);
    }

    fn lb_zoom(&mut self, factor: f32, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        let s = clampf(lb.scale * factor, 0.1, 5.);
        let k = s / lb.scale;
        lb.scale = s;
        lb.tx *= k;
        lb.ty *= k;
        lb.orig = false;
        self.lb_clamp();
        cx.notify();
    }

    fn lb_go(&mut self, d: isize, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        let ix = lb.ix as isize + d;
        if ix < 0 || ix as usize >= lb.ids.len() {
            return;
        }
        lb.ix = ix as usize;
        lb.rot = 0;
        lb.need_fit = true;
        lb.orig = false;
        cx.notify();
    }

    fn lb_toggle(&mut self, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        if lb.orig && (lb.scale - 1.).abs() < 0.01 {
            self.lb_fit();
        } else {
            lb.scale = 1.;
            lb.tx = 0.;
            lb.ty = 0.;
            lb.orig = true;
            self.lb_clamp();
        }
        cx.notify();
    }

    fn lb_rotate(&mut self, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        lb.rot = (lb.rot + 1) % 4;
        lb.tx = 0.;
        lb.ty = 0.;
        self.lb_clamp();
        cx.notify();
    }

    fn lb_show_chrome(&mut self, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        lb.last_move = Instant::now();
        if !lb.chrome {
            lb.chrome = true;
            lb.chrome_t = Instant::now();
            cx.notify();
        }
        self.img.chrome_task = Some(cx.spawn(async |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |e, cx| {
                if let Some(lb) = &mut e.img.lightbox
                    && lb.chrome
                    && lb.last_move.elapsed() >= Duration::from_millis(4990)
                {
                    lb.chrome = false;
                    lb.chrome_t = Instant::now();
                    cx.notify();
                }
            });
        }));
    }

    /// 下载: saves the shown (cropped) image.
    fn lb_download(&mut self, cx: &mut Context<Self>) {
        let Some(lb) = &self.img.lightbox else { return };
        let Some((path, crop, _, _)) = lb.ids.get(lb.ix).and_then(|id| self.lb_source(*id)) else { return };
        let fmt = fdl_core::vfs::File::open(&path).ok()
            .and_then(|f| image::ImageReader::new(std::io::BufReader::new(f)).with_guessed_format().ok())
            .and_then(|r| r.format());
        let ext = fmt.and_then(|f| f.extensions_str().first().copied()).unwrap_or("png");
        #[cfg(target_family = "wasm")]
        {
            let result = if crop == FULL {
                fdl_core::vfs::read(&path).ok().and_then(|bytes| {
                    crate::web_download::download_file(&format!("image.{ext}"), &bytes, fmt.map_or("application/octet-stream", |f| f.to_mime_type())).ok()
                })
            } else {
                shrunk(&path, 0).ok().and_then(|(im, _, _)| {
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image_crop(im, crop).write_to(&mut bytes, image::ImageFormat::Png).ok()?;
                    crate::web_download::download_file("image.png", &bytes.into_inner(), "image/png").ok()
                })
            };
            if result.is_none() { self.toast_kind(ToastKind::Error, crate::settings::text("导出失败，请重试"), cx); }
        }
        #[cfg(not(target_family = "wasm"))]
        {
        let dir = dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("image.{ext}")));
        let bg = cx.background_executor().clone();
        cx.spawn(async move |_, _| {
            let Ok(Ok(Some(dest))) = rx.await else { return };
            bg.spawn(async move {
                if crop == FULL {
                    let _ = fdl_core::vfs::copy(&path, &dest);
                } else if let Ok(im) = image::ImageReader::open(&path).and_then(|r| r.with_guessed_format()).map_err(anyhow::Error::from).and_then(|r| Ok(r.decode()?)) {
                    let _ = image_crop(im, crop).save(&dest);
                }
            })
            .await;
        })
        .detach();
        }
    }

    /// Decodes the rotated pixels of the current image off the main thread.
    fn lb_ensure_rotated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(lb) = &mut self.img.lightbox else { return };
        let Some(&id) = lb.ids.get(lb.ix) else { return };
        let rot = lb.rot;
        if rot == 0 || lb.rotated.as_ref().is_some_and(|(r, i, _)| *r == rot && *i == id) {
            return;
        }
        if let Some((_, _, Some(old))) = lb.rotated.take() {
            let _ = window.drop_image(old);
        }
        lb.rotated = Some((rot, id, None));
        let Some((path, crop, _, _)) = self.lb_source(id) else { return };
        let task = cx.background_executor().spawn(async move { decode_rotated(&path, crop, rot) });
        cx.spawn_in(window, async move |this, cx| {
            let Some(im) = task.await else { return };
            let _ = this.update_in(cx, |e, window, cx| match &mut e.img.lightbox {
                Some(lb) if matches!(lb.rotated, Some((r, i, None)) if r == rot && i == id) => {
                    lb.rotated = Some((rot, id, Some(im)));
                    cx.notify();
                }
                _ => {
                    let _ = window.drop_image(im);
                }
            });
        })
        .detach();
    }

    /// Animations, the fit and the end of the closing fade.
    pub(super) fn tick_image_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.img.toast.as_ref().is_some_and(|(_, _, t)| t.elapsed() >= Duration::from_secs(3)) {
            self.img.toast = None;
        }
        if !cx.has_active_drag() {
            self.img.file_drop = None;
        }
        // The image stays selected (toolbar, outline) while its description
        // is typed in. gpui's focus listeners don't see fields inside list
        // rows, so this follows the focus here instead.
        let typing = self
            .captions
            .iter()
            .find(|(id, (f, _))| f.read(cx).focus_handle(cx).is_focused(window) && matches!(self.core.kind_of(**id), Some(BlockKind::Image { .. })))
            .map(|(id, _)| *id);
        if let Some(id) = self.img.caption_focus
            && typing != Some(id)
        {
            self.caption_blurred(id, cx);
        }
        self.img.caption_focus = typing;
        let Some(lb) = &mut self.img.lightbox else { return };
        if lb.closing.is_some_and(|t| t.elapsed().as_secs_f32() >= FADE) {
            if let Some((_, _, Some(im))) = lb.rotated.take() {
                let _ = window.drop_image(im);
            }
            self.img.lightbox = None;
            self.img.chrome_task = None;
            return;
        }
        let vp = window.viewport_size();
        let vp = (f32::from(vp.width), f32::from(vp.height));
        if lb.vp != vp {
            lb.vp = vp;
            lb.need_fit = true;
        }
        let animating = lb.opened.elapsed().as_secs_f32() < FADE || lb.closing.is_some() || lb.chrome_t.elapsed().as_secs_f32() < FADE;
        let cur = lb.ids.get(lb.ix).copied();
        if cur.is_none_or(|id| self.lb_source(id).is_none()) {
            // The image went away (undo, another edit).
            if let Some(lb) = &mut self.img.lightbox
                && lb.closing.is_none()
            {
                lb.closing = Some(Instant::now());
                window.focus(&self.focus, cx);
            }
            window.request_animation_frame();
            return;
        }
        if self.img.lightbox.as_ref().is_some_and(|l| l.need_fit) {
            self.lb_fit();
        }
        self.lb_ensure_rotated(window, cx);
        if animating {
            window.request_animation_frame();
        }
    }

    pub(super) fn lightbox_el(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lb = self.img.lightbox.as_ref()?;
        let id = *lb.ids.get(lb.ix)?;
        let (path, crop, _, _) = self.lb_source(id)?;
        let (w, h) = self.lb_size()?;
        let (vw, vh) = lb.vp;
        let fade = match lb.closing {
            Some(t) => 1. - (t.elapsed().as_secs_f32() / FADE).min(1.),
            None => ease_out((lb.opened.elapsed().as_secs_f32() / FADE).min(1.)),
        };
        let k = ease_out((lb.chrome_t.elapsed().as_secs_f32() / FADE).min(1.));
        let chrome = if lb.chrome { k } else { 1. - k };
        // Idle with the controls faded out: no pointer until the mouse moves.
        if !lb.chrome && k >= 1. { cx.hide_cursor_until_mouse_moves(); }
        let (sw, sh) = (w * lb.scale, h * lb.scale);
        let (left, top) = (vw / 2. + lb.tx - sw / 2., vh / 2. + lb.ty - sh / 2.);
        let can_drag = sw > vw + 0.5 || sh > vh + 0.5;
        let pic = if lb.rot == 0 {
            let (frw, frh) = (crop[2] - crop[0], crop[3] - crop[1]);
            let (fw, fh) = (sw / frw, sh / frh);
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(sw))
                .h(px(sh))
                .overflow_hidden()
                .bg(gpui::white())
                .child(
                    picture(&path, fw.max(fh))
                        .absolute()
                        .left(px(-crop[0] * fw))
                        .top(px(-crop[1] * fh))
                        .w(px(fw))
                        .h(px(fh))
                        .object_fit(ObjectFit::Fill)
                        .with_loading(|| {
                            div().size_full().flex().items_center().justify_center().child(spinning("lb-loading", 40., theme::rgbaf(255, 255, 255, 0.6), 1.4)).into_any_element()
                        })
                        .id(("lb-img", id.0)),
                )
        } else {
            let base = div().absolute().left(px(left)).top(px(top)).w(px(sw)).h(px(sh));
            match &lb.rotated {
                Some((_, _, Some(im))) => base.bg(gpui::white()).child(img(im.clone()).size_full().object_fit(ObjectFit::Fill)),
                _ => base.flex().items_center().justify_center().child(spinning("lb-loading", 40., theme::rgbaf(255, 255, 255, 0.6), 1.4)),
            }
        };
        let pic = div()
            .id("lb-pic")
            .absolute()
            .inset_0()
            .child(pic.id("lb-pic-box").when(can_drag, |d| d.cursor(if lb.press.is_some() { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })));

        let white = gpui::white();
        // Chrome over the dark backdrop: near-black glass, white glyphs, in either look (literal
        // colours: `ca` would turn the glass near-white in the dark under the same white glyphs).
        let close = div()
            .id("lb-close")
            .absolute()
            .top(px(20.))
            .right(px(20.))
            .size(px(40.))
            .rounded_full()
            .bg(theme::rgbaf(0x14, 0x15, 0x17, 0.6))
            .hover(|s| s.bg(theme::rgbaf(0x30, 0x33, 0x35, 0.75)))
            .border_1()
            .border_color(theme::rgbaf(255, 255, 255, 0.1))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .opacity(chrome)
            .child(icon("x", 20., white))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|e, _, window, cx| {
                    cx.stop_propagation();
                    e.close_lightbox(window, cx);
                }),
            );
        let tool = |key: &'static str, ic: &'static str, tip: &'static str, enabled: bool, f: fn(&mut Editor, &mut Window, &mut Context<Editor>)| {
            div()
                .id(key)
                .size(px(28.))
                .mx(px(4.))
                .rounded(px(ROW_RADIUS))
                .flex()
                .items_center()
                .justify_center()
                .tip_above(tip, None)
                .child(icon(ic, 18., white))
                .map(|d| {
                    if enabled {
                        d.cursor_pointer().hover(|s| s.bg(theme::rgbaf(255, 255, 255, 0.16))).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |e, _, window, cx| {
                                cx.stop_propagation();
                                f(e, window, cx);
                            }),
                        )
                    } else {
                        d.opacity(0.4)
                    }
                })
        };
        let sep = || div().w(px(1.)).h(px(18.)).mx(px(8.)).my(px(2.)).bg(theme::rgbaf(255, 255, 255, 0.2));
        let label = |s: String, min: f32| div().min_w(px(min)).flex().justify_center().text_size(px(13.)).line_height(px(20.)).text_color(white).child(s);
        let n = lb.ids.len();
        let fitted = lb.orig && (lb.scale - 1.).abs() < 0.01;
        // The controls: a dark glass pill, rounded like the header pills.
        let pill = div()
            .id("lb-pill")
            .h(px(44.))
            .px(px(10.))
            .rounded(px(12.))
            .bg(theme::rgbaf(0x14, 0x15, 0x17, 0.78))
            .border_1()
            .border_color(theme::rgbaf(255, 255, 255, 0.1))
            .shadow(theme::shadow_panel())
            .flex()
            .items_center()
            .font_family(theme::ui_family())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(tool("lb-prev", "chevron-left", crate::settings::text("上一张"), lb.ix > 0, |e, _, cx| e.lb_go(-1, cx)))
            .child(label(format!("{}/{}", lb.ix + 1, n), 0.))
            .child(tool("lb-next", "chevron-right", crate::settings::text("下一张"), lb.ix + 1 < n, |e, _, cx| e.lb_go(1, cx)))
            .child(sep())
            .child(tool("lb-out", "zoom-out", crate::settings::text("缩小"), lb.scale > 0.1 + 1e-4, |e, _, cx| e.lb_zoom(1. / 1.15, cx)))
            .child(label(format!("{}%", (lb.scale * 100.).round() as i32), 40.))
            .child(tool("lb-in", "zoom-in", crate::settings::text("放大"), lb.scale < 5. - 1e-4, |e, _, cx| e.lb_zoom(1.15, cx)))
            .child(if fitted {
                tool("lb-size", "fit-page", crate::settings::text("适应页面"), true, |e, _, cx| e.lb_toggle(cx))
            } else {
                tool("lb-size", "one-to-one", crate::settings::text("原始尺寸"), true, |e, _, cx| e.lb_toggle(cx))
            })
            .child(sep())
            .child(tool("lb-rotate", "rotate-ccw-square", crate::settings::text("旋转"), true, |e, _, cx| e.lb_rotate(cx)))
            .child(tool("lb-download", "download", crate::settings::text("下载"), true, |e, _, cx| e.lb_download(cx)));
        let pill = div().absolute().bottom(px(20.)).left_0().w(px(vw)).flex().justify_center().opacity(chrome).child(pill);

        let layer = div()
            .id("lightbox")
            .key_context("Lightbox")
            .track_focus(&self.img.lb_focus)
            .occlude()
            .relative()
            .w(px(vw))
            .h(px(vh))
            .bg(theme::rgbaf(0, 0, 0, 0.8))
            .opacity(fade)
            .on_action(cx.listener(|e, _: &LbClose, window, cx| e.close_lightbox(window, cx)))
            .on_action(cx.listener(|e, _: &LbPrev, _, cx| e.lb_go(-1, cx)))
            .on_action(cx.listener(|e, _: &LbNext, _, cx| e.lb_go(1, cx)))
            .on_action(cx.listener(|e, _: &LbZoomIn, _, cx| e.lb_zoom(1.15, cx)))
            .on_action(cx.listener(|e, _: &LbZoomOut, _, cx| e.lb_zoom(1. / 1.15, cx)))
            .on_action(cx.listener(|e, _: &LbToggle, _, cx| e.lb_toggle(cx)))
            .on_action(cx.listener(|e, _: &LbFit, _, cx| {
                e.lb_fit();
                if let Some(lb) = &mut e.img.lightbox {
                    lb.orig = false;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|e, _: &LbRotate, _, cx| e.lb_rotate(cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|e, ev: &MouseDownEvent, _, cx| {
                    if let Some(lb) = &mut e.img.lightbox {
                        lb.press = Some((ev.position, lb.tx, lb.ty));
                        lb.moved = false;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|e, ev: &MouseMoveEvent, _, cx| {
                e.lb_show_chrome(cx);
                let can_drag = e.lb_size().zip(e.img.lightbox.as_ref()).is_some_and(|((w, h), lb)| w * lb.scale > lb.vp.0 + 0.5 || h * lb.scale > lb.vp.1 + 0.5);
                let Some(lb) = &mut e.img.lightbox else { return };
                let Some((from, tx, ty)) = lb.press else { return };
                if ev.pressed_button != Some(MouseButton::Left) {
                    lb.press = None;
                    return cx.notify();
                }
                let d = ev.position - from;
                if f32::from(d.x).hypot(f32::from(d.y)) > 5. {
                    lb.moved = true;
                }
                if can_drag {
                    lb.tx = tx + f32::from(d.x);
                    lb.ty = ty + f32::from(d.y);
                    e.lb_clamp();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|e, _: &MouseUpEvent, window, cx| {
                    let Some(lb) = &mut e.img.lightbox else { return };
                    // A click without a drag, on the image or around it, closes.
                    if lb.press.take().is_some() && !lb.moved {
                        e.close_lightbox(window, cx);
                    }
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(|e, ev: &ScrollEvent, _, cx| {
                let (dx, dy, lines) = match ev.delta {
                    ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y), false),
                    ScrollDelta::Lines(l) => (l.x, l.y, true),
                };
                let (dx, dy) = if lines { (dx * 150., dy * 150.) } else { (dx, dy) };
                if ev.modifiers.control || ev.modifiers.platform {
                    let f = 1. + dy.abs() / if ev.modifiers.control { 100. } else { 400. };
                    e.lb_zoom(if dy > 0. { f } else { 1. / f }, cx);
                } else if let Some(lb) = &mut e.img.lightbox {
                    lb.tx += 3. * dx;
                    lb.ty += 3. * dy;
                    e.lb_clamp();
                    cx.notify();
                }
            }))
            .child(pic)
            .child(close)
            .child(pill);
        let _ = window;
        Some(deferred(anchored().position_mode(AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(layer)).with_priority(10).into_any_element())
    }

    // ---- toast

    /// The error toast (image-spec, paste and link failures).
    pub(super) fn toast(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.toast_kind(ToastKind::Error, msg, cx);
    }

    pub fn clear_toast(&mut self, cx: &mut Context<Self>) {
        if self.img.toast.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn toast_kind(&mut self, kind: ToastKind, msg: &str, cx: &mut Context<Self>) {
        self.img.toast = Some((kind, SharedString::from(msg.to_string()), Instant::now()));
        // One toast at a time: this one replaces the workspace's.
        cx.emit(super::view::EditorEvent::Toast);
        self.img.toast_task = Some(cx.spawn(async |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2700)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
        cx.notify();
    }

    pub(super) fn toast_el(&self, window: &mut Window) -> Option<AnyElement> {
        let (kind, msg, t0) = self.img.toast.as_ref()?;
        crate::editor_ui::toast_el(*kind, msg, *t0, window)
    }

    // ---- inserting

    /// Dragged files over the page: the drop line follows the pointer.
    pub(super) fn file_drag_move(&mut self, ev: &DragMoveEvent<ExternalPaths>, _: &mut Window, cx: &mut Context<Self>) {
        let p = ev.event.position;
        let d = if ev.bounds.contains(&p) { self.file_drop_at(p) } else { None };
        if d != self.img.file_drop {
            self.img.file_drop = d;
            cx.notify();
        }
    }

    pub(super) fn file_drop_line(&self) -> Option<AnyElement> {
        let (_, _, l) = self.img.file_drop?;
        let o = self.content_origin.get();
        // The drop indicator: a 2pt accent line, its ends rounded.
        Some(div().absolute().left(l.left() - o.x).top(l.top() - o.y).w(l.size.width).h(px(2.)).rounded(px(1.)).bg(Theme::primary()).into_any_element())
    }

    pub(super) fn drop_files(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.img.file_drop.take();
        let fallback = || {
            let h = self.core.sel.head.block;
            match self.core.doc.blocks.first() {
                Some(first) if h == TITLE => Some((first.id, DropPlace::Before)),
                _ if self.core.block(h).is_some() => Some((h, DropPlace::After)),
                _ => self.core.doc.blocks.last().map(|b| (b.id, DropPlace::After)),
            }
        };
        let target = target.map(|(t, p, _)| (t, p)).or_else(fallback);
        // Images become image blocks, anything else an attachment after them.
        let (pics, others): (Vec<&PathBuf>, Vec<&PathBuf>) = paths.paths().iter().partition(|p| {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
            FORMATS.contains(&ext.as_str())
        });
        let mut blocks = vec![];
        let mut err = None;
        for p in pics {
            match image_block_from_file(p) {
                Ok(b) => blocks.push(b),
                Err(e) => err = Some(e),
            }
        }
        if let Some(e) = err {
            self.toast(e, cx);
        }
        if !others.is_empty() {
            let after = blocks.last().map(|b| (b.id, DropPlace::After)).or(target);
            self.import_files(others.into_iter().cloned().collect(), after, window, cx);
        }
        let Some(last) = blocks.last().map(|b| b.id) else { return cx.notify() };
        match target {
            Some((t, place)) => self.core.insert_blocks_at(blocks, t, place),
            None => {
                self.core.checkpoint(EditKind::Other);
                self.core.doc.blocks.extend(blocks);
            }
        }
        self.core.clear_block_sel();
        self.core.cell_sel = None;
        self.core.sel = Selection::caret(Pos::new(last, 0));
        window.focus(&self.focus, cx);
        self.changed(window, cx);
    }

    // ---- remote images

    /// Fetches queued remote images into the cache (`curl`, off the main thread).
    pub(super) fn start_downloads(&mut self, cx: &mut Context<Self>) {
        let queued: Vec<String> = self.img.remote.borrow().iter().filter(|(_, s)| **s == Remote::Queued).map(|(u, _)| u.clone()).collect();
        for url in queued {
            self.img.remote.borrow_mut().insert(url.clone(), Remote::Pending);
            let dest = self.img.remote_path(&url);
            let u = url.clone();
            let task = cx.background_executor().spawn(async move { download(&u, &dest) });
            cx.spawn(async move |this, cx| {
                let ok = task.await;
                let _ = this.update(cx, |e, cx| {
                    if ok {
                        e.img.remote.borrow_mut().remove(&url);
                        e.adopt_remote(cx);
                    } else {
                        e.img.remote.borrow_mut().insert(url, Remote::Failed);
                    }
                    e.img.generation += 1;
                    cx.notify();
                });
            })
            .detach();
        }
    }
}

/// Web images pasted this session (#220), with their copy in the assets
/// folder once made. Links like Feishu's expire within hours, so a pasted
/// one becomes an owned asset as soon as it is downloaded. Process-wide: the
/// document may be closed before the download lands, and is relinked the
/// next time an editor opens it.
static PASTED: std::sync::Mutex<Vec<(String, Option<PathBuf>)>> = std::sync::Mutex::new(Vec::new());

impl Editor {
    /// Queues pasted web images for download now (not only once scrolled
    /// to) and adopts any already cached. With 网络图片 turned off they stay
    /// links; a failed download too.
    pub(super) fn adopt_pasted(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        if urls.is_empty() || !crate::settings::prefs().remote_images { return }
        {
            let mut pasted = PASTED.lock().unwrap_or_else(|e| e.into_inner());
            for url in urls {
                self.img.remote.borrow_mut().entry(url.clone()).or_insert(Remote::Queued);
                if !pasted.iter().any(|(u, _)| *u == url) { pasted.push((url, None)); }
            }
        }
        self.adopt_remote(cx);
        // Queued downloads start on the next frame.
        cx.notify();
    }

    /// Points this document's pasted web images at their downloaded copies,
    /// copying a download into the assets folder the first time (not an undo
    /// step: the undo history is relinked too). Saves like any other edit.
    pub fn adopt_remote(&mut self, cx: &mut Context<Self>) {
        if self.read_only { return }
        let mut pasted = PASTED.lock().unwrap_or_else(|e| e.into_inner());
        if pasted.is_empty() { return }
        let mut srcs = std::collections::HashSet::new();
        self.core.doc.for_each_block(|b| if let BlockKind::Image { src, .. } = &b.kind && src.starts_with("http") { srcs.insert(src.clone()); });
        let mut changed = false;
        pasted.retain_mut(|(url, copy)| {
            if !srcs.contains(url.as_str()) { return true }
            if copy.is_none() {
                let Some(cached) = cached_remote(url) else { return true };
                // Not an image we can keep (too big, odd format): it stays a link.
                let Some(p) = adopt_file(&cached) else { return false };
                *copy = Some(p);
            }
            let to = copy.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            changed |= self.core.relink_image(url, &to);
            true
        });
        drop(pasted);
        // Right after a paste the caller's `changed()` reports it.
        if changed && !self.core.dirty {
            cx.emit(EditorEvent::Changed);
            cx.notify();
        }
    }
}

/// The `http(s)` image sources among pasted blocks, for `adopt_pasted`.
pub(super) fn pasted_remote(blocks: &[Block]) -> Vec<String> {
    let mut out = vec![];
    remote_srcs(blocks, &mut out);
    out
}

fn remote_srcs(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        remote_srcs(&b.children, out);
        if let BlockKind::Table(t) = &b.kind {
            for cell in t.rows.iter().flatten() { remote_srcs(&cell.blocks, out); }
        }
        if let BlockKind::Image { src, .. } = &b.kind
            && (src.starts_with("http://") || src.starts_with("https://")) && !out.contains(src) {
            out.push(src.clone());
        }
    }
}

/// A downloaded remote image (no extension in the cache) as a new asset.
fn adopt_file(cached: &Path) -> Option<PathBuf> {
    let bytes = fdl_core::vfs::read(cached).ok()?;
    let ext = match image::guess_format(&bytes).ok()? {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Bmp => "bmp",
        _ => return None,
    };
    store_image(&bytes, ext).ok()
}

fn download(url: &str, dest: &Path) -> bool {
    if let Some(d) = dest.parent() {
        let _ = fdl_core::vfs::create_dir_all(d);
    }
    let tmp = dest.with_extension("part");
    let mut curl = std::process::Command::new("curl");
    curl.args(["-fsSL", "--max-time", "60", "-o"])
        .arg(&tmp)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // No console window flashing up on Windows (#415).
    #[cfg(target_os = "windows")]
    { use std::os::windows::process::CommandExt; curl.creation_flags(0x0800_0000); }
    let fetched = curl.status().is_ok_and(|s| s.success());
    let ok = fetched
        && image::ImageReader::open(&tmp).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.into_dimensions().ok()).is_some()
        && fdl_core::vfs::rename(&tmp, dest).is_ok();
    if !ok {
        let _ = fdl_core::vfs::remove_file(&tmp);
    }
    ok
}

fn image_crop(im: image::DynamicImage, crop: [f32; 4]) -> image::DynamicImage {
    if crop == FULL { return im; }
    let (w, h) = (im.width(), im.height());
    let x = ((crop[0] * w as f32).round() as u32).min(w - 1);
    let y = ((crop[1] * h as f32).round() as u32).min(h - 1);
    let cw = (((crop[2] - crop[0]) * w as f32).round() as u32).clamp(1, w - x);
    let ch = (((crop[3] - crop[1]) * h as f32).round() as u32).clamp(1, h - y);
    im.crop_imm(x, y, cw, ch)
}

/// Crops and turns the image `rot` quarter turns counter-clockwise (BGRA).
fn decode_rotated(path: &Path, crop: [f32; 4], rot: u8) -> Option<Arc<RenderImage>> {
    let (im, _, _) = shrunk(path, 0).ok()?;
    let im = image_crop(im, crop);
    let im = match rot {
        1 => im.rotate270(),
        2 => im.rotate180(),
        3 => im.rotate90(),
        _ => im,
    };
    let mut buf = im.into_rgba8();
    for px in buf.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
}

/// A new image block for pasted or picked bytes, stored in the assets
/// folder (format, size and pixel checks on upload).
pub(super) fn image_block_from(bytes: &[u8], ext: &str) -> Result<Block, &'static str> { image_block_named(&Store::assets_dir(), bytes, ext, None) }

/// macOS screenshots may only offer TIFF. Store those as lossless PNG;
/// other clipboard formats keep their original bytes (including animation).
pub(crate) fn clipboard_image_data(img: &gpui::Image) -> Result<(std::borrow::Cow<'_, [u8]>, &'static str), &'static str> {
    if img.bytes.len() as u64 > MAX_BYTES { return Err(TOO_BIG); }
    let ext = match img.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Webp => "webp",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Svg | ImageFormat::Ico | ImageFormat::Pnm => return Err(BAD_FORMAT),
        ImageFormat::Tiff => {
            let im = image::ImageReader::with_format(std::io::Cursor::new(&img.bytes), image::ImageFormat::Tiff)
                .decode().map_err(|_| BAD_DIMS)?;
            let mut png = std::io::Cursor::new(Vec::new());
            im.write_to(&mut png, image::ImageFormat::Png).map_err(|_| BAD_DIMS)?;
            return Ok((std::borrow::Cow::Owned(png.into_inner()), "png"));
        }
    };
    Ok((std::borrow::Cow::Borrowed(&img.bytes), ext))
}

/// `image_block_from` for a file that has a name of its own, which its copy keeps.
/// The picture copied into `assets`, as an image block.
fn image_block_named(assets: &Path, bytes: &[u8], ext: &str, name: Option<&str>) -> Result<Block, &'static str> {
    let path = store_image_in(assets, bytes, ext, name)?;
    let mut b = Block::new(
        BlockKind::Image { src_kind: "src".into(), src: path.to_string_lossy().to_string(), width: None, height: None, caption: None, crop: None },
        RichText::default(),
    );
    b.align = Align::Center;
    Ok(b)
}

/// Checks picked or pasted image bytes and copies them into the assets folder.
pub(crate) fn store_image(bytes: &[u8], ext: &str) -> Result<PathBuf, &'static str> { store_image_named(bytes, ext, None) }

/// `store_image`; an image with a `name` (a picked or dropped file) is kept
/// as `<8 hex>-<name>.<ext>`, so its name survives in the library and in the
/// copy beside a Markdown file. A pasted one is just `<uuid>.<ext>`.
pub(crate) fn store_image_named(bytes: &[u8], ext: &str, name: Option<&str>) -> Result<PathBuf, &'static str> { store_image_in(&Store::assets_dir(), bytes, ext, name) }

/// `store_image_named` into `assets`: an import names the library it writes to, never the one in front.
fn store_image_in(assets: &Path, bytes: &[u8], ext: &str, name: Option<&str>) -> Result<PathBuf, &'static str> {
    let ext = ext.to_ascii_lowercase();
    if !FORMATS.contains(&ext.as_str()) {
        return Err(BAD_FORMAT);
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TOO_BIG);
    }
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .ok_or(BAD_DIMS)?;
    let id = uuid::Uuid::new_v4().to_string();
    let name: Option<String> = name.map(|n| n.chars().map(|c| if matches!(c, '/' | '\\' | ':') || c.is_control() { '_' } else { c }).take(80).collect::<String>())
        .map(|n| n.trim().trim_start_matches('.').to_string()).filter(|n| !n.is_empty());
    let file = match name { Some(n) => format!("{}-{n}.{ext}", &id[..8]), None => format!("{id}.{ext}") };
    let _ = fdl_core::vfs::create_dir_all(assets);
    let path = assets.join(file);
    fdl_core::vfs::write(&path, bytes).map_err(|_| BAD_DIMS)?;
    Ok(path)
}

pub(crate) fn image_block_from_file(p: &Path) -> Result<Block, &'static str> { image_block_from_file_in(&Store::assets_dir(), p) }

/// `image_block_from_file`, copied into `assets`.
pub(crate) fn image_block_from_file_in(assets: &Path, p: &Path) -> Result<Block, &'static str> {
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !p.fs_is_file() || !FORMATS.contains(&ext.as_str()) {
        return Err(BAD_FORMAT);
    }
    if fdl_core::vfs::metadata(p).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        return Err(TOO_BIG);
    }
    let bytes = fdl_core::vfs::read(p).map_err(|_| BAD_DIMS)?;
    let b = image_block_named(assets, &bytes, &ext, p.file_stem().and_then(|s| s.to_str()))?;
    // The copy in the assets keeps the picture's change time, as a copy in the Finder does.
    if let (BlockKind::Image { src, .. }, Some((_, changed))) = (&b.kind, fdl_core::vfs::file_times(p)) { let _ = fdl_core::vfs::set_modified(Path::new(src), changed); }
    Ok(b)
}

/// One line of pasted text that is nothing but a local image file path
/// (`file://` counts): the file to take in, if it is really an image
/// that exists. WeChat puts such a line in the pasteboard instead of
/// image data (#252).
pub(crate) fn local_image_path(line: &str) -> Option<PathBuf> {
    let p = local_file(line.trim())?;
    is_local_image_file(&p).then_some(p)
}

/// The file a picture's source names on this computer, if it is an absolute
/// path: `/…`, Windows' `C:\…`, `C:/…` or `\\server\…`, or a `file://` URL
/// (`file:///C:/…` on Windows). None for a URL, `data:` or a relative path.
pub(crate) fn local_file(src: &str) -> Option<PathBuf> {
    let decoded = src.strip_prefix("file://").map(crate::markdown::percent_decode);
    let s = decoded.as_deref().unwrap_or(src);
    #[cfg(windows)]
    let s = if decoded.is_some() && s.starts_with('/') && s.as_bytes().get(2) == Some(&b':') { &s[1..] } else { s };
    let p = Path::new(s);
    (p.fs_is_absolute() && (decoded.is_some() || !src.contains("://"))).then(|| p.to_path_buf())
}

pub(super) fn is_local_image_file(p: &Path) -> bool {
    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
    p.is_absolute() && FORMATS.contains(&ext.as_str()) && p.fs_is_file()
}

/// File name of a remote image's download in `assets/remote`.
fn remote_name(url: &str) -> String {
    let mut h = rustc_hash::FxHasher::default();
    url.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The downloaded copy of a remote image, if it has been fetched.
pub fn cached_remote(url: &str) -> Option<PathBuf> {
    Some(Store::assets_dir().join("remote").join(remote_name(url))).filter(|p| p.fs_is_file())
}

/// Pasted HTML images: `data:` URIs and local files become owned assets;
/// unreadable ones are dropped. Remote images stay remote.
pub(crate) fn localize_pasted_images(blocks: &mut Vec<Block>) { localize_images_in(&Store::assets_dir(), blocks) }

/// `localize_pasted_images` into `assets` (an import's library's).
pub(crate) fn localize_images_in(assets: &Path, blocks: &mut Vec<Block>) {
    use base64::Engine;
    blocks.retain_mut(|b| {
        localize_images_in(assets, &mut b.children);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() {
                localize_images_in(assets, &mut cell.blocks);
                if cell.blocks.is_empty() && !cell.is_covered() {
                    cell.blocks.push(Block::text(""));
                }
            }
        }
        let BlockKind::Image { src, .. } = &b.kind else { return true };
        let local = if let Some(data) = src.strip_prefix("data:image/") {
            let Some((head, payload)) = data.split_once(',') else { return false };
            let ext = head.split(';').next().unwrap_or_default().replace("jpeg", "jpg");
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload.trim()) else { return false };
            image_block_named(assets, &bytes, &ext, None)
        } else if let Some(file) = local_file(src) {
            // Any absolute path, Windows' `C:\…` too: never left pointing at the folder it came from.
            image_block_from_file_in(assets, &file)
        } else {
            return true;
        };
        // The copy keeps the width and caption the page gave the picture (`<img width>`, `![[a.png|300]]`).
        let (old_width, old_caption) = match &b.kind { BlockKind::Image { width, caption, .. } => (*width, caption.clone()), _ => (None, None) };
        match local {
            Ok(n) => {
                b.kind = n.kind;
                if let BlockKind::Image { width, height, caption, .. } = &mut b.kind {
                    if old_width.is_some() { (*width, *height) = (old_width, None); }
                    if old_caption.is_some() { *caption = old_caption; }
                }
                true
            }
            Err(_) => false,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{cap, clipboard_image_data, decode, decode_png, local_file, local_image_path, pasted_remote, BAD_DIMS};
    use crate::model::{Block, BlockKind, RichText};
    use gpui::ImageFormat;

    fn img(src: &str) -> Block {
        Block::new(BlockKind::Image { src_kind: "src".into(), src: src.into(), width: None, height: None, caption: None, crop: None }, RichText::default())
    }

    /// #220: pasted web images to fetch, nested ones too, each once; local and inline ones stay out.
    #[test]
    fn finds_pasted_remote_images() {
        let mut quote = Block::text("q");
        quote.children = vec![img("https://f.cn/a?code=1"), img("/tmp/x.png")];
        let blocks = vec![img("http://e.com/b.png"), quote, img("data:image/png;base64,AAAA"), img("https://f.cn/a?code=1")];
        assert_eq!(pasted_remote(&blocks), ["http://e.com/b.png", "https://f.cn/a?code=1"]);
    }

    fn photo(dir: &std::path::Path, w: u32, h: u32) -> std::path::PathBuf {
        let p = dir.join(format!("{w}x{h}.jpg"));
        let im = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8]));
        image::DynamicImage::ImageRgb8(im).save_with_format(&p, image::ImageFormat::Jpeg).unwrap();
        p
    }

    #[test]
    fn caps_round_up_in_steps() {
        assert_eq!((cap(0.), cap(1.), cap(386.), cap(1380.), cap(2017.), cap(9000.)), (0, 128, 512, 1536, 2048, 0));
    }

    /// Shrunk at decode (the JPEG at most 15% under the cap), never above it;
    /// the hollow copy keeps id and size.
    #[test]
    fn pictures_decode_at_their_shown_size() {
        let dir = fdl_core::vfs::temp_dir().join(format!("qmd-pic-{}", fdl_core::vfs::process_id()));
        fdl_core::vfs::create_dir_all(&dir).unwrap();
        let p = photo(&dir, 4032, 3024);
        for (c, long) in [(2048, 2016), (512, 504), (384, 384)] {
            let im = decode(&p, c).unwrap();
            let s = im.size(0);
            assert_eq!(s.width.0 as u32, long, "cap {c}");
            assert!(s.width.0 as u32 <= c && s.width.0 as f32 >= c as f32 * 0.85);
            let h = im.hollow().unwrap();
            assert!(h.is_hollow() && h.id == im.id && h.size(0) == s && h.as_bytes(0).is_none());
        }
        // 0 keeps every pixel; a PNG shrinks through `image`.
        assert_eq!(decode(&p, 0).unwrap().size(0).width.0, 4032);
        let png = dir.join("wide.png");
        image::RgbaImage::new(3000, 1000).save(&png).unwrap();
        assert_eq!(decode(&png, 1024).unwrap().size(0).width.0, 1024);
        let _ = fdl_core::vfs::remove_dir_all(&dir);
    }

    /// PNGs shrink row by row to the same size as `thumbnail_exact`, and close
    /// to its pixels, whatever the color type; small or interlaced ones are
    /// left to `image`.
    #[test]
    fn pngs_shrink_while_streaming() {
        let dir = fdl_core::vfs::temp_dir().join(format!("qmd-png-{}", fdl_core::vfs::process_id()));
        fdl_core::vfs::create_dir_all(&dir).unwrap();
        let px = |x: u32, y: u32| image::Rgba([(x * 255 / 2999) as u8, (y * 255 / 1199) as u8, ((x + y) % 256) as u8, (255 - y * 255 / 1199) as u8]);
        let rgba = image::RgbaImage::from_fn(3000, 1200, px);
        let opaque = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
        let gray = image::DynamicImage::ImageRgba8(rgba.clone()).to_luma8();
        let cases: [(&str, image::DynamicImage); 3] = [("rgba", rgba.into()), ("rgb", opaque.into()), ("gray", gray.into())];
        for (name, im) in cases {
            let p = dir.join(format!("{name}.png"));
            im.save(&p).unwrap();
            let (got, w, h) = decode_png(&p, 1024).unwrap();
            assert_eq!((w, h, got.width(), got.height()), (3000, 1200, 1024, 410), "{name}");
            let want = im.thumbnail_exact(1024, 410).into_rgba8();
            let got = got.into_rgba8();
            // Away from the box edges (the filters split them differently).
            for (x, y) in [(10, 10), (500, 200), (1000, 400), (300, 50)] {
                let (a, b) = (got.get_pixel(x, y).0, want.get_pixel(x, y).0);
                let close = (0..4).all(|c| (a[c] as i32 - b[c] as i32).abs() <= 4 || (c < 3 && a[3] < 8));
                assert!(close, "{name} at {x},{y}: {a:?} vs {b:?}");
            }
            assert_eq!(decode(&p, 1024).unwrap().size(0).width.0, 1024);
        }
        assert!(decode_png(&dir.join("rgb.png"), 4096).is_none());
        assert!(decode_png(&dir.join("missing.png"), 512).is_none());
        let _ = fdl_core::vfs::remove_dir_all(&dir);
    }

    #[test]
    fn local_image_path_takes_only_existing_image_files() {
        let dir = std::env::temp_dir().join(format!("fdl-imgpath-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("shot.png");
        std::fs::write(&png, b"not decoded, only named").unwrap();
        let shown = png.display().to_string();
        assert_eq!(local_image_path(&shown).as_deref(), Some(png.as_path()));
        assert_eq!(local_image_path(&format!("  file://{shown}  ")).as_deref(), Some(png.as_path()));
        // Not there, wrong type, or relative: text, not an image.
        assert_eq!(local_image_path(&dir.join("gone.png").display().to_string()), None);
        assert_eq!(local_image_path(&dir.join("notes.txt").display().to_string()), None);
        assert_eq!(local_image_path("shot.png"), None);
        assert_eq!(local_image_path("https://example.com/a.png"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clipboard_tiff_preserves_pixels_as_png() {
        let pixels = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 80, 255, 128]).unwrap();
        let mut tiff = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels.clone()).write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();
        let img = gpui::Image::from_bytes(ImageFormat::Tiff, tiff.into_inner());
        let (png, ext) = clipboard_image_data(&img).unwrap();
        assert_eq!(ext, "png");
        assert_eq!(image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().into_rgba8(), pixels);
    }

    #[test]
    fn clipboard_invalid_tiff_is_rejected() {
        let img = gpui::Image::from_bytes(ImageFormat::Tiff, b"broken tiff".to_vec());
        assert_eq!(clipboard_image_data(&img).unwrap_err(), BAD_DIMS);
    }

    #[test]
    fn clipboard_gif_keeps_original_bytes() {
        let img = gpui::Image::from_bytes(ImageFormat::Gif, b"GIF89a".to_vec());
        let (bytes, ext) = clipboard_image_data(&img).unwrap();
        assert_eq!(ext, "gif");
        assert!(matches!(bytes, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*bytes, img.bytes);
    }

    /// An absolute path is a local file on every platform's own terms (Windows'
    /// `C:\…` and `C:/…` too), so an import copies it in; URLs and relative paths are not.
    #[test]
    fn local_file_takes_every_absolute_path() {
        let here = std::env::temp_dir().join("pic.png");
        assert_eq!(local_file(&here.to_string_lossy()), Some(here.clone()));
        for no in ["https://e.com/a.png", "data:image/png;base64,AAAA", "pics/a.png", "./a.png", "..\\a.png"] { assert_eq!(local_file(no), None, "{no}"); }
        #[cfg(unix)]
        assert_eq!(local_file("file:///tmp/a%20b.png"), Some(std::path::PathBuf::from("/tmp/a b.png")));
        #[cfg(windows)]
        for (src, want) in [(r"C:\Users\a\pic.png", r"C:\Users\a\pic.png"), ("C:/Users/a/pic.png", "C:/Users/a/pic.png"), (r"\\server\share\pic.png", r"\\server\share\pic.png"), ("file:///C:/Users/a%20b/pic.png", "C:/Users/a b/pic.png")] {
            assert_eq!(local_file(src), Some(std::path::PathBuf::from(want)), "{src}");
        }
    }
}
