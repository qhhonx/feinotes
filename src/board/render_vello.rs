//! The vello_cpu backend of the board's display list (docs/design/whiteboard-
//! excalidraw.md, 渲染方案 2; every platform alike): the visible elements are
//! drawn by vello_cpu (multithreaded) straight into a BGRA buffer and shown
//! as a new GPUI image each time the picture changes; the buffer comes back
//! from the atlas two frames later and is drawn into again. `render.rs` picks
//! it above a number of visible elements; overlay and chrome stay GPUI.
//!
//! Same geometry and colors as the GPUI backend: paths from `display.rs`
//! (kurbo, cached per element version in `Cached::vello`), colors through
//! `render::color`. Frame clips and arrow label holes are clip layers. Text
//! is shaped by GPUI (Excalidraw's fonts, `fonts::runs`) on upstream's baseline
//! and drawn as glyph outlines (`WindowTextSystem::glyph_outline`: CoreText,
//! DirectWrite, swash);
//! color emoji have no outline and are left out. Images are sampled from the
//! decoded file (`render_geom::Src`), rotated, mirrored and cropped by the
//! paint transform.

use crate::board::display::{self as dl, DisplayItem, ElementDisplay, ImageItem, Paint};
use super::geom::{Pt, Src};
use crate::board::shape::PathCmd;
use crate::board::types::TextAlign;
use super::{Cached, VText, runs};
use gpui::{Bounds, Corners, FontId, GlyphId, Hsla, Pixels, RenderImage, Window, px};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::sync::Arc;
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, BezPath, Cap, Join, Rect, Shape, Stroke};
use vello_cpu::peniko::{Fill, ImageAlphaType, ImageSampler};
use vello_cpu::{Image, ImageSource, PixelMetadata, Pixmap, RenderContext, Resources};

/// One element's paths, local units, in item order (fill before stroke).
pub type Paths = Vec<BezPath>;

pub struct Vello {
    ctx: RenderContext,
    res: Resources,
    /// The last rendered frame, BGRA (GPUI's image layout), drawn 1:1.
    frame: Option<Arc<RenderImage>>,
    /// Frames shown before, their atlas tiles freed at the next `show`.
    stale: Vec<Arc<RenderImage>>,
    /// A shown frame's pixels, back from the atlas, for the next frame.
    spare: Vec<u8>,
    size: (u16, u16),
    /// Scene → device pixels of the canvas, and its zoom.
    view: Affine,
    zoom: f64,
    /// Glyph outlines in em units, y down, by font and glyph.
    glyphs: FxHashMap<(FontId, GlyphId), Option<BezPath>>,
    upem: FxHashMap<FontId, f64>,
    /// Premultiplied image pixels by file id, with the cap they were decoded at.
    images: HashMap<String, (u32, Option<Arc<Pixmap>>)>,
}

fn rgba(h: Hsla) -> AlphaColor<Srgb> { let c = gpui::Rgba::from(h); AlphaColor::new([c.r, c.g, c.b, c.a]) }

fn kpt(p: [f64; 2]) -> (f64, f64) { (p[0], p[1]) }

/// `d`'s paths as kurbo paths in local units.
pub fn build(d: &ElementDisplay) -> Paths {
    let mut out = vec![];
    for it in &d.items {
        let DisplayItem::Path { cmds, fill, stroke } = it else { continue };
        let mut p = BezPath::new();
        for c in cmds {
            match *c {
                PathCmd::MoveTo(a) => p.move_to(kpt(a)),
                PathCmd::LineTo(a) => p.line_to(kpt(a)),
                PathCmd::QuadTo(a, b) => p.quad_to(kpt(a), kpt(b)),
                PathCmd::CubicTo(a, b, c2) => p.curve_to(kpt(a), kpt(b), kpt(c2)),
                PathCmd::Close => p.close_path(),
            }
        }
        for _ in 0..(fill.is_some() as usize + stroke.is_some() as usize) { out.push(p.clone()); }
    }
    out
}

fn poly_path(poly: &[Pt]) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in poly.iter().enumerate() { if i == 0 { p.move_to(kpt(*q)) } else { p.line_to(kpt(*q)) } }
    p.close_path();
    p
}

impl Vello {
    pub fn new() -> Self {
        Vello { ctx: RenderContext::new(1, 1), res: Resources::new(), frame: None, stale: vec![], spare: vec![], size: (0, 0), view: Affine::IDENTITY, zoom: 1.,
            glyphs: FxHashMap::default(), upem: FxHashMap::default(), images: HashMap::new() }
    }

    /// Starts a frame of `w` × `h` device pixels filled with `bg`; `view`
    /// maps scene units to them.
    pub fn begin(&mut self, w: u16, h: u16, bg: Hsla, view: Affine, zoom: f64) {
        self.zoom = zoom;
        if self.size != (w, h) {
            self.size = (w, h);
            self.ctx = RenderContext::new(w, h);
        } else {
            self.ctx.reset();
        }
        self.view = view;
        self.ctx.reset_transform();
        self.ctx.set_paint(rgba(bg));
        self.ctx.fill_rect(&Rect::new(0., 0., w as f64, h as f64));
    }

    /// Draws one element. `clip`: its frame clip (scene polygon); `fade`:
    /// the dim factor; `src` finds image pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn element(&mut self, c: &mut Cached, d: &ElementDisplay, fade: f32, canvas: Hsla, src: &mut dyn FnMut(&ImageItem, f64, Option<u32>) -> super::Take, window: &Window) {
        let Vello { ctx, res: _, view, glyphs, upem, images, zoom, .. } = self;
        let zoom = *zoom;
        let el = *view * Affine::translate(kpt(d.origin)) * Affine::rotate_about(d.angle, kpt(d.center));
        let mut layers = 0;
        if let Some(k) = &c.clip {
            ctx.set_transform(*view);
            ctx.set_fill_rule(Fill::NonZero);
            ctx.push_clip_layer(&poly_path(k));
            layers += 1;
        }
        if fade < 1. { ctx.push_opacity_layer(fade); layers += 1; }
        if let Some([x, y, w, h]) = d.knockout {
            // everything but the label box: a big box with the label as a hole
            let b = c.bounds;
            let big = 4. * ((b[2] - b[0]).abs() + (b[3] - b[1]).abs() + w + h) + 1e4;
            let mut p = Rect::new(-big, -big, big, big).to_path(0.1);
            p.extend(Rect::new(x, y, x + w, y + h).to_path(0.1));
            ctx.set_transform(el);
            ctx.set_fill_rule(Fill::EvenOdd);
            ctx.push_clip_layer(&p);
            layers += 1;
        }
        let paint_of = |p: Paint| super::paint_color(p, d, canvas);
        if c.vello.is_none() { c.vello = Some(build(d)); }
        let paths = c.vello.as_ref().expect("built");
        let mut k = 0;
        for (i, it) in d.items.iter().enumerate() {
            match it {
                DisplayItem::Path { fill, stroke, .. } => {
                    ctx.set_transform(el);
                    if let Some(f) = fill {
                        ctx.set_fill_rule(if f.rule == dl::FillRule::EvenOdd { Fill::EvenOdd } else { Fill::NonZero });
                        ctx.set_paint(rgba(paint_of(f.paint)));
                        if let Some(p) = paths.get(k) { ctx.fill_path(p); }
                        k += 1;
                    }
                    if let Some(s) = stroke {
                        let cap = match s.cap { dl::LineCap::Butt => Cap::Butt, dl::LineCap::Round => Cap::Round, dl::LineCap::Square => Cap::Square };
                        let join = match s.join { dl::LineJoin::Miter => Join::Miter, dl::LineJoin::Round => Join::Round, dl::LineJoin::Bevel => Join::Bevel };
                        let mut st = Stroke::new(s.width.max(0.01)).with_caps(cap).with_join(join);
                        if let Some(dash) = &s.dash { st = st.with_dashes(0., dash.iter().map(|v| v.max(0.01))); }
                        ctx.set_stroke(st);
                        ctx.set_paint(rgba(paint_of(s.paint)));
                        if let Some(p) = paths.get(k) { ctx.stroke_path(p); }
                        k += 1;
                    }
                }
                DisplayItem::Text(t) => {
                    if c.vtext.len() <= i { c.vtext.resize_with(d.items.len(), || None); }
                    // shaped at the size on screen, as the GPUI backend does (SF's tracking varies with size)
                    let z = zoom as f32;
                    if c.vtext[i].as_ref().is_none_or(|v| v.0 != z.to_bits()) { c.vtext[i] = Some((z.to_bits(), shape(t, z, window))); }
                    let shaped = &c.vtext[i].as_ref().expect("shaped").1;
                    ctx.set_fill_rule(Fill::NonZero);
                    ctx.set_paint(rgba(paint_of(t.color)));
                    let size = t.font_size;
                    for (font, gs) in shaped.iter() {
                        let u = *upem.entry(*font).or_insert_with(|| window.text_system().units_per_em(*font).max(1) as f64);
                        for (g, at) in gs {
                            let Some(o) = glyphs.entry((*font, *g)).or_insert_with(|| outline(window, *font, *g, u)) else { continue };
                            ctx.set_transform(el * Affine::translate((at[0], at[1])) * Affine::scale(size));
                            ctx.fill_path(o);
                        }
                    }
                }
                DisplayItem::Image(img) => {
                    let [x, y, w, h] = img.dest;
                    let flip = Affine::translate((x + w / 2., y + h / 2.)) * Affine::scale_non_uniform(img.flip[0], img.flip[1]) * Affine::translate((-x - w / 2., -y - h / 2.));
                    ctx.set_transform(el * flip);
                    let shape = if img.corner_radius > 0. { vello_cpu::kurbo::RoundedRect::new(x, y, x + w, y + h, img.corner_radius).to_path(0.1) } else { Rect::new(x, y, x + w, y + h).to_path(0.1) };
                    let need = super::need_px(img, zoom * window.scale_factor() as f64);
                    let pm = img.file_id.as_ref().and_then(|id| {
                        // decoded again only when the cap changes (thumbnails)
                        if let super::Take::Ready(cap, s) = src(img, need, images.get(id).map(|(c, _)| *c)) { images.insert(id.clone(), (cap, s.and_then(pixmap_of))); }
                        images.get(id)?.1.clone()
                    });
                    ctx.set_fill_rule(Fill::NonZero);
                    match pm {
                        Some(pm) => {
                            let (pw, ph) = (pm.width() as f64, pm.height() as f64);
                            let pt = match (img.crop, img.natural_size) {
                                (Some(cr), Some([nw, nh])) if cr[2] > 0. && cr[3] > 0. && nw > 0. && nh > 0. => {
                                    let (kx, ky) = (pw / nw, ph / nh);
                                    Affine::translate((x, y)) * Affine::scale_non_uniform(w / (cr[2] * kx), h / (cr[3] * ky)) * Affine::translate((-cr[0] * kx, -cr[1] * ky))
                                }
                                _ => Affine::translate((x, y)) * Affine::scale_non_uniform(w / pw, h / ph),
                            };
                            ctx.set_paint_transform(pt);
                            ctx.set_paint(Image { image: ImageSource::Pixmap(pm), sampler: ImageSampler { alpha: d.opacity as f32, ..ImageSampler::default() } });
                            ctx.fill_path(&shape);
                            ctx.reset_paint_transform();
                        }
                        None => {
                            ctx.set_paint(rgba(super::placeholder_color(d.opacity)));
                            ctx.fill_path(&shape);
                        }
                    }
                }
            }
        }
        for _ in 0..layers { ctx.pop_layer(); }
    }

    /// Renders the frame into a new image: vello_cpu draws straight into
    /// the buffer of a frame shown before (back from the atlas), RGBA
    /// premultiplied (the canvas is opaque), swapped to BGRA in place.
    pub fn render(&mut self) {
        self.ctx.flush();
        let (w, h) = self.size;
        let mut buf = std::mem::take(&mut self.spare);
        buf.resize(w as usize * h as usize * 4, 0);
        let Some(target) = vello_cpu::PixmapMut::new(w, h, &mut buf) else { return };
        self.ctx.render(target, &mut self.res);
        for p in buf.as_chunks_mut::<4>().0 { let v = u32::from_le_bytes(*p); *p = (v & 0xff00_ff00 | (v >> 16 & 0xff) | (v & 0xff) << 16).to_le_bytes(); }
        let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, buf) else { return };
        if let Some(old) = self.frame.replace(Arc::new(RenderImage::new(vec![image::Frame::new(img)]))) { self.stale.push(old); }
    }

    /// Paints the last rendered frame at `bounds` (the canvas).
    pub fn show(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        for old in self.stale.drain(..) {
            let _ = window.drop_image(old.clone());
            #[cfg(not(target_os = "ios"))]
            if let Some(f) = Arc::into_inner(old).and_then(|i| i.into_frames().into_iter().next()) { self.spare = f.into_buffer().into_raw(); }
        }
        if let Some(frame) = &self.frame {
            // device pixels to logical, so the frame lands 1:1
            let sf = window.scale_factor();
            let size = gpui::size(px(self.size.0 as f32 / sf), px(self.size.1 as f32 / sf));
            let at = Bounds::new(bounds.origin, size);
            let _ = window.paint_image(at, at, Corners::default(), frame.clone(), 0, false);
        }
    }

}

fn pixmap_of(s: Src) -> Option<Arc<Pixmap>> {
    let (w, h) = (u16::try_from(s.w).ok()?, u16::try_from(s.h).ok()?);
    Some(Arc::new(Pixmap::from_parts(s.px, w, h, PixelMetadata { may_have_transparency: true, alpha_type: ImageAlphaType::Alpha })))
}

/// A glyph outline in em units, y down.
fn outline(window: &Window, font: FontId, g: GlyphId, upem: f64) -> Option<BezPath> {
    use gpui::GlyphOutlineCmd as C;
    let cmds = window.text_system().glyph_outline(font, g)?;
    let q = |p: [f32; 2]| (p[0] as f64 / upem, -p[1] as f64 / upem);
    let mut b = BezPath::new();
    for c in cmds {
        match c { C::MoveTo(p) => b.move_to(q(p)), C::LineTo(p) => b.line_to(q(p)), C::QuadTo(a, p) => b.quad_to(q(a), q(p)), C::CubicTo(a, a2, p) => b.curve_to(q(a), q(a2), q(p)), C::Close => b.close_path() }
    }
    (!b.elements().is_empty()).then_some(b)
}

/// A text run shaped by GPUI at its scene size: per font, glyphs with
/// their local baseline origin (lines placed as `render::paint_text` does:
/// on upstream's baseline).
fn shape(t: &dl::TextRun, zoom: f32, window: &Window) -> VText {
    let zoom = zoom.max(1e-3);
    let size = (t.font_size as f32 * zoom).max(1.);
    let k = (t.font_size as f32 / size) as f64;
    let mut out: VText = vec![];
    for l in &t.lines {
        if l.text.is_empty() { continue; }
        let lay = window.text_system().layout_line(&l.text, px(size), &runs(&l.text, t.font_family, gpui::black()), None);
        let w = f32::from(lay.width) as f64 * k;
        let x = match t.align { TextAlign::Left => l.x, TextAlign::Center => l.x - w / 2., TextAlign::Right => l.x - w };
        let base = l.baseline;
        for r in &lay.runs {
            let gs = r.glyphs.iter().filter(|g| !g.is_emoji).map(|g| (g.id, [x + f32::from(g.position.x) as f64 * k, base + f32::from(g.position.y) as f64 * k]));
            match out.last_mut() { Some((f, v)) if *f == r.font_id => v.extend(gs), _ => out.push((r.font_id, gs.collect())) }
        }
    }
    out
}
