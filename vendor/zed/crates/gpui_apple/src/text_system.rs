use anyhow::anyhow;
use collections::{HashMap, HashSet};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    base::{CGFloat, CGGlyph, kCGImageAlphaPremultipliedLast},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::CGPoint,
};
use core_text::{
    font::CTFont,
    font_collection::CTFontCollectionRef,
    font_descriptor::{
        CTFontDescriptor, kCTFontDefaultOrientation, kCTFontItalicTrait, kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontWeightTrait,
    },
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};
use font_kit::{
    font::Font as FontKitFont,
    handle::Handle,
    metrics::Metrics,
    properties::{Style as FontkitStyle, Weight as FontkitWeight},
    source::SystemSource,
    sources::mem::MemSource,
};
use gpui::{
    Bounds, DevicePixels, Font, FontFallbacks, FontFeatures, FontId, FontMetrics, FontRun,
    FontStyle, FontWeight, GlyphId, LineLayout, Pixels, PlatformTextSystem, RenderGlyphParams,
    Result, SUBPIXEL_VARIANTS_X, ShapedGlyph, ShapedRun, SharedString, Size, TextRenderingMode,
    point, px, size, swap_rgba_pa_to_bgra,
};
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    vector::Vector2F,
};
use smallvec::SmallVec;
use std::{borrow::Cow, char, convert::TryFrom, sync::Arc};

#[cfg(target_os = "macos")]
use {
    core_foundation::base::CFType,
    gpui::Hsla,
    std::sync::OnceLock,
};

use crate::open_type::apply_features_and_fallbacks;

#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

/// Apple text system using CoreText for font shaping.
pub struct AppleTextSystem(RwLock<AppleTextSystemState>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    font_family: SharedString,
    font_features: FontFeatures,
    font_fallbacks: Option<FontFallbacks>,
}

struct AppleTextSystemState {
    memory_source: MemSource,
    system_source: SystemSource,
    fonts: Vec<FontKitFont>,
    font_selections: HashMap<Font, FontId>,
    /// quiet.md: several faces can share a PostScript name with different
    /// glyph tables (CoreText's fallback copy of PingFang SC vs. the family
    /// in PingFang.ttc), so keep them all and tell them apart by glyph count.
    font_ids_by_postscript_name: HashMap<String, SmallVec<[FontId; 2]>>,
    font_ids_by_font_key: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    postscript_names_by_font_id: HashMap<FontId, String>,
    /// quiet.md: synthetic italics. Twins of upright faces that draw sheared,
    /// for italic text in a family without an italic face (PingFang), as
    /// browsers do.
    oblique: HashSet<FontId>,
    oblique_twins: HashMap<FontId, FontId>,
    /// quiet.md: each font at each size it was laid out in. CoreText keeps its
    /// shaping tables, ASCII advances and fallback faces on the CTFont object,
    /// so a fresh copy per line set them all up again: most of the time spent
    /// laying out text not seen before (scrolling a long document).
    sized: HashMap<(FontId, u32), CTFont>,
    /// quiet.md: the face CoreText fell back to for a CJK character a sized
    /// font has no glyph for, as CoreText made it (weight and all), given to
    /// later lines instead of letting it fall back again: every line made new
    /// fallback CTFonts and set up their shaping tables again, a quarter of
    /// laying out Chinese text.
    cjk_fallbacks: HashMap<(FontId, u32, char), CTFont>,
}

/// quiet.md: characters whose fallback face is kept (`cjk_fallbacks`): Han,
/// kana, CJK and full-width punctuation. They stand alone (no combining
/// marks, variation selectors or emoji sequences, which are left to CoreText).
fn is_cjk_char(ch: char) -> bool {
    matches!(ch, '\u{2E80}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FE10}'..='\u{FE1F}' | '\u{FE30}'..='\u{FE4F}' | '\u{FF00}'..='\u{FFEF}')
}

/// WebKit's synthetic oblique, 14°.
const OBLIQUE_SKEW: f32 = 0.2493;

impl AppleTextSystem {
    /// Create a new AppleTextSystem.
    pub fn new() -> Self {
        Self(RwLock::new(AppleTextSystemState {
            memory_source: MemSource::empty(),
            system_source: SystemSource::new(),
            fonts: Vec::new(),
            font_selections: HashMap::default(),
            font_ids_by_postscript_name: HashMap::default(),
            font_ids_by_font_key: HashMap::default(),
            postscript_names_by_font_id: HashMap::default(),
            oblique: HashSet::default(),
            oblique_twins: HashMap::default(),
            sized: HashMap::default(),
            cjk_fallbacks: HashMap::default(),
        }))
    }
}

impl Default for AppleTextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextSystem for AppleTextSystem {
    // quiet.md: outlines for the board's CPU renderer (synthetic italics
    // sheared like their raster).
    fn glyph_outline(&self, font_id: FontId, glyph_id: GlyphId) -> Option<Vec<gpui::GlyphOutlineCmd>> {
        use font_kit::{hinting::HintingOptions, outline::OutlineSink};
        use gpui::GlyphOutlineCmd as C;
        use pathfinder_geometry::line_segment::LineSegment2F;
        struct Sink(Vec<C>, f32);
        impl Sink {
            fn p(&self, v: Vector2F) -> [f32; 2] {
                [v.x() + v.y() * self.1, v.y()]
            }
        }
        impl OutlineSink for Sink {
            fn move_to(&mut self, to: Vector2F) {
                let p = self.p(to);
                self.0.push(C::MoveTo(p));
            }
            fn line_to(&mut self, to: Vector2F) {
                let p = self.p(to);
                self.0.push(C::LineTo(p));
            }
            fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
                let (a, b) = (self.p(ctrl), self.p(to));
                self.0.push(C::QuadTo(a, b));
            }
            fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
                let (a, b, c) = (self.p(ctrl.from()), self.p(ctrl.to()), self.p(to));
                self.0.push(C::CubicTo(a, b, c));
            }
            fn close(&mut self) {
                self.0.push(C::Close);
            }
        }
        let state = self.0.read();
        let font = state.fonts.get(font_id.0)?;
        let skew = if state.oblique.contains(&font_id) { OBLIQUE_SKEW } else { 0. };
        let mut sink = Sink(vec![], skew);
        font.outline(glyph_id.0, HintingOptions::None, &mut sink).ok()?;
        Some(sink.0)
    }

    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let collection = core_text::font_collection::create_for_all_families();
        // NOTE: We intentionally avoid using `collection.get_descriptors()` here because
        // it has a memory leak bug in core-text v21.0.0. The upstream code uses
        // `wrap_under_get_rule` but `CTFontCollectionCreateMatchingFontDescriptors`
        // follows the Create Rule (caller owns the result), so it should use
        // `wrap_under_create_rule`. We call the function directly with correct memory management.
        unsafe extern "C" {
            fn CTFontCollectionCreateMatchingFontDescriptors(
                collection: CTFontCollectionRef,
            ) -> CFArrayRef;
        }
        let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
            let array_ref =
                CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
            if array_ref.is_null() {
                None
            } else {
                Some(CFArray::wrap_under_create_rule(array_ref))
            }
        };
        let Some(descriptors) = descriptors else {
            return names;
        };
        for descriptor in descriptors.into_iter() {
            names.extend(lenient_font_attributes::family_name(&descriptor));
        }
        if let Ok(fonts_in_memory) = self.0.read().memory_source.all_families() {
            names.extend(fonts_in_memory);
        }
        names
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.0.upgradable_read();
        if let Some(font_id) = lock.font_selections.get(font) {
            Ok(*font_id)
        } else {
            let mut lock = RwLockUpgradableReadGuard::upgrade(lock);
            let font_key = FontKey {
                font_family: font.family.clone(),
                font_features: font.features.clone(),
                font_fallbacks: font.fallbacks.clone(),
            };
            let candidates = if let Some(font_ids) = lock.font_ids_by_font_key.get(&font_key) {
                font_ids.as_slice()
            } else {
                let font_ids =
                    lock.load_family(&font.family, &font.features, font.fallbacks.as_ref())?;
                lock.font_ids_by_font_key.insert(font_key.clone(), font_ids);
                lock.font_ids_by_font_key[&font_key].as_ref()
            };

            let candidate_properties = candidates
                .iter()
                .map(|font_id| font_properties(&lock.fonts[font_id.0]))
                .collect::<SmallVec<[_; 4]>>();

            let ix = font_kit::matching::find_best_match(
                &candidate_properties,
                &font_kit::properties::Properties {
                    style: fontkit_style(font.style),
                    weight: fontkit_weight(font.weight),
                    stretch: Default::default(),
                },
            )?;

            let mut font_id = candidates[ix];
            if font.style != FontStyle::Normal && candidate_properties[ix].style == FontkitStyle::Normal {
                font_id = lock.oblique_twin(font_id);
            }
            lock.font_selections.insert(font.clone(), font_id);
            Ok(font_id)
        }
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        font_kit_metrics_to_metrics(self.0.read().fonts[font_id.0].metrics())
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        Ok(bounds_from_rect(
            self.0.read().fonts[font_id.0].typographic_bounds(glyph_id.0)?,
        ))
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.0.read().raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        glyph_id: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.0.read().rasterize_glyph(glyph_id, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, font_runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }

    #[cfg(target_os = "macos")]
    fn glyph_dilation_for_color(&self, _color: Hsla) -> u8 {
        // quiet.md: one darkening level for every colour (see rasterize_glyph),
        // so glyphs of any colour share one rasterization.
        0
    }
}

#[cfg(target_os = "macos")]
fn font_smoothing_allowed_by_user() -> bool {
    static ALLOWED: OnceLock<bool> = OnceLock::new();
    *ALLOWED.get_or_init(|| {
        use core_foundation_sys::preferences::{
            CFPreferencesCopyAppValue, kCFPreferencesCurrentApplication,
        };

        let key = CFString::new("AppleFontSmoothing");
        let value_ref = unsafe {
            CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), kCFPreferencesCurrentApplication)
        };
        if value_ref.is_null() {
            return true;
        }
        let value = unsafe { CFType::wrap_under_create_rule(value_ref) };
        let Some(number) = value.downcast_into::<CFNumber>() else {
            return true;
        };
        // Only an explicit value of `0` means that font smoothing is disabled.
        number.to_i64() != Some(0)
    })
}

impl AppleTextSystemState {
    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let fonts = fonts
            .into_iter()
            .map(|bytes| match bytes {
                Cow::Borrowed(embedded_font) => {
                    let data_provider = unsafe {
                        core_graphics::data_provider::CGDataProvider::from_slice(embedded_font)
                    };
                    let font = core_graphics::font::CGFont::from_data_provider(data_provider)
                        .map_err(|()| anyhow!("Could not load an embedded font."))?;
                    let font = font_kit::loaders::core_text::Font::from_core_graphics_font(font);
                    Ok(Handle::from_native(&font))
                }
                Cow::Owned(bytes) => Ok(Handle::from_memory(Arc::new(bytes), 0)),
            })
            .collect::<Result<Vec<_>>>()?;
        self.memory_source.add_fonts(fonts.into_iter())?;
        Ok(())
    }

    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[FontId; 4]>> {
        let name = gpui::font_name_with_fallbacks(name, ".AppleSystemUIFont");

        let mut font_ids = SmallVec::new();
        let mut postscript_names_seen = HashSet::default();
        let family = self
            .memory_source
            .select_family_by_name(name)
            .or_else(|_| self.system_source.select_family_by_name(name))?;
        for font in family.fonts() {
            let mut font = font.load()?;

            apply_features_and_fallbacks(&mut font, features, fallbacks)?;
            // This block contains a precautionary fix to guard against loading fonts
            // that might cause panics due to `.unwrap()`s up the chain.
            {
                // We use the 'm' character for text measurements in various spots
                // (e.g., the editor). However, at time of writing some of those usages
                // will panic if the font has no 'm' glyph.
                //
                // Therefore, we check up front that the font has the necessary glyph.
                let has_m_glyph = font.glyph_for_char('m').is_some();

                // HACK: The 'Segoe Fluent Icons' font does not have an 'm' glyph,
                // but we need to be able to load it for rendering Windows icons in
                // the Storybook (on macOS).
                let is_segoe_fluent_icons = font.full_name() == "Segoe Fluent Icons";
                // quiet.md: KaTeX's symbol faces (Size1-4, AMS, Caligraphic,
                // Script) have no lowercase letters.
                let is_katex = font.full_name().starts_with("KaTeX_");
                // quiet.md: nor does the board's CJK-only Xiaolai.
                let is_xiaolai = font.full_name().starts_with("Xiaolai");

                if !has_m_glyph && !is_segoe_fluent_icons && !is_katex && !is_xiaolai {
                    // I spent far too long trying to track down why a font missing the 'm'
                    // character wasn't loading. This log statement will hopefully save
                    // someone else from suffering the same fate.
                    log::warn!(
                        "font '{}' has no 'm' character and was not loaded",
                        font.full_name()
                    );
                    continue;
                }
            }

            // We've seen a number of panics in production caused by calling font.properties()
            // which unwraps a downcast to CFNumber. This is an attempt to avoid the panic,
            // and to try and identify the incalcitrant font.
            // quiet.md: font.properties() unwraps four trait numbers. iOS leaves
            // some out (Helvetica, Arial), so they are read with defaults
            // (font_properties) instead of skipping the face.
            if font.native_font().all_traits().get(unsafe { kCTFontSymbolicTrait }).downcast::<CFNumber>().is_none() {
                log::warn!("font {:?} has no symbolic traits; reading defaults", font.full_name());
            }

            let Some(postscript_name) = font.postscript_name() else {
                log::warn!(
                    "font {:?} in family {:?} has no PostScript name; skipping",
                    font.full_name(),
                    name,
                );
                continue;
            };
            // Dedup is scoped to this single `load_family` call (issue #55472).
            // The same family can be reloaded later under a different `FontKey`
            // (different features/fallbacks); a global check against
            // `font_ids_by_postscript_name` would skip every already-registered
            // font and leave the second call's `font_ids` empty.
            if !postscript_names_seen.insert(postscript_name.clone()) {
                log::warn!(
                    "skipping duplicate font {:?} with PostScript name {:?} \
                     in family {:?}",
                    font.full_name(),
                    postscript_name,
                    name,
                );
                continue;
            }
            let font_id = FontId(self.fonts.len());
            font_ids.push(font_id);
            self.font_ids_by_postscript_name
                .entry(postscript_name.clone())
                .or_default()
                .push(font_id);
            self.postscript_names_by_font_id
                .insert(font_id, postscript_name);
            self.fonts.push(font);
        }
        Ok(font_ids)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        Ok(size_from_vector2f(
            self.fonts[font_id.0].advance(glyph_id.0)?,
        ))
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.fonts[font_id.0].glyph_for_char(ch).map(GlyphId)
    }

    fn id_for_native_font(&mut self, requested_font: CTFont) -> FontId {
        let postscript_name = requested_font.postscript_name();
        let glyph_count = requested_font.glyph_count() as u32;
        if let Some(font_id) = self
            .font_ids_by_postscript_name
            .get(&postscript_name)
            .and_then(|ids| ids.iter().find(|id| self.fonts[id.0].glyph_count() == glyph_count))
        {
            *font_id
        } else {
            let font_id = FontId(self.fonts.len());
            self.font_ids_by_postscript_name
                .entry(postscript_name.clone())
                .or_default()
                .push(font_id);
            self.postscript_names_by_font_id
                .insert(font_id, postscript_name);
            self.fonts
                .push(font_kit::font::Font::from_core_graphics_font(
                    requested_font.copy_to_CGFont(),
                ));
            font_id
        }
    }

    fn oblique_twin(&mut self, font_id: FontId) -> FontId {
        if let Some(&twin) = self.oblique_twins.get(&font_id) {
            return twin;
        }
        let twin = FontId(self.fonts.len());
        self.fonts.push(self.fonts[font_id.0].clone());
        if let Some(name) = self.postscript_names_by_font_id.get(&font_id).cloned() {
            self.postscript_names_by_font_id.insert(twin, name);
        }
        self.oblique.insert(twin);
        self.oblique_twins.insert(font_id, twin);
        twin
    }

    fn is_italic(&self, font_id: FontId) -> bool {
        // quiet.md: font_properties, not font-kit's properties(): iOS leaves the
        // slant number out of some fallback faces' traits and properties() unwraps it.
        self.oblique.contains(&font_id) || font_properties(&self.fonts[font_id.0]).style != FontkitStyle::Normal
    }

    fn is_emoji(&self, font_id: FontId) -> bool {
        self.postscript_names_by_font_id
            .get(&font_id)
            .is_some_and(|postscript_name| {
                postscript_name == "AppleColorEmoji" || postscript_name == ".AppleColorEmojiUI"
            })
    }

    fn raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        // quiet.md: measure with the same sized CTFont that rasterize_glyph
        // draws with. font-kit scales bounds taken at its base size, but the
        // system font switches optical size (SF Text → SF Display) with the
        // point size, so large title glyphs came out wider than their bitmap
        // and lost a sliver on the left.
        let rect = self.fonts[params.font_id.0]
            .native_font()
            .clone_with_font_size(f32::from(params.font_size) as CGFloat)
            .get_bounding_rects_for_glyphs(kCTFontDefaultOrientation, &[params.glyph_id.0 as CGGlyph]);
        // A sheared glyph reaches further right at the top, left below the baseline.
        let mut rect = rect;
        if self.oblique.contains(&params.font_id) {
            let k = OBLIQUE_SKEW as CGFloat;
            let (x0, x1) = (rect.origin.x + k * rect.origin.y, rect.origin.x + rect.size.width + k * (rect.origin.y + rect.size.height));
            rect.origin.x = x0;
            rect.size.width = x1 - x0;
        }
        // Flip to "origin is top left", as font-kit does.
        let rect = RectF::new(
            Vector2F::new(rect.origin.x as f32, -(rect.origin.y + rect.size.height) as f32),
            Vector2F::new(rect.size.width as f32, rect.size.height as f32),
        );
        let bounds: Bounds<DevicePixels> = bounds_from_rect_i((rect * params.scale_factor).round_out().to_i32());

        // Expand the bounds by 1 pixel on each side to give CG room for anti-aliasing.
        Ok(bounds.dilate(DevicePixels(1)))
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        } else {
            // Add an extra pixel when the subpixel variant isn't zero to make room for anti-aliasing.
            let mut bitmap_size = glyph_bounds.size;
            if params.subpixel_variant.x > 0 {
                bitmap_size.width += DevicePixels(1);
            }
            if params.subpixel_variant.y > 0 {
                bitmap_size.height += DevicePixels(1);
            }
            let bitmap_size = bitmap_size;

            let mut bytes;
            let cx;
            if params.is_emoji {
                bytes = vec![0; bitmap_size.width.0 as usize * 4 * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize * 4,
                    &CGColorSpace::create_device_rgb(),
                    kCGImageAlphaPremultipliedLast,
                );
            } else {
                bytes = vec![0; bitmap_size.width.0 as usize * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize,
                    &CGColorSpace::create_device_gray(),
                    kCGImageAlphaOnly,
                );
            }

            // Move the origin to bottom left and account for scaling, this
            // makes drawing text consistent with the font-kit's raster_bounds.
            cx.translate(
                -glyph_bounds.origin.x.0 as CGFloat,
                (glyph_bounds.origin.y.0 + glyph_bounds.size.height.0) as CGFloat,
            );
            cx.scale(
                params.scale_factor as CGFloat,
                params.scale_factor as CGFloat,
            );

            let subpixel_shift = params
                .subpixel_variant
                .map(|v| v as f32 / SUBPIXEL_VARIANTS_X as f32);
            cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
            cx.set_allows_antialiasing(true);
            cx.set_should_antialias(true);
            cx.set_allows_font_subpixel_positioning(true);
            cx.set_should_subpixel_position_fonts(true);
            cx.set_allows_font_subpixel_quantization(false);
            cx.set_should_subpixel_quantize_fonts(false);

            // quiet.md: every colour is smoothed (stem darkening) at the
            // darkest level, as the app measured against native AppKit /
            // Catalyst text; only the user's AppleFontSmoothing = 0 turns it off.
            #[cfg(target_os = "macos")]
            if font_smoothing_allowed_by_user() {
                cx.set_allows_font_smoothing(true);
                cx.set_should_smooth_fonts(true);
            }
            cx.set_gray_fill_color(0.0, 1.0);
            if self.oblique.contains(&params.font_id) {
                cx.concat_ctm(core_graphics::geometry::CGAffineTransform::new(1., 0., OBLIQUE_SKEW as CGFloat, 1., 0., 0.));
            }
            self.fonts[params.font_id.0]
                .native_font()
                .clone_with_font_size(f32::from(params.font_size) as CGFloat)
                .draw_glyphs(
                    &[params.glyph_id.0 as CGGlyph],
                    &[CGPoint::new(
                        (subpixel_shift.x / params.scale_factor) as CGFloat,
                        (subpixel_shift.y / params.scale_factor) as CGFloat,
                    )],
                    cx,
                );

            if params.is_emoji {
                // Convert from RGBA with premultiplied alpha to BGRA with straight alpha.
                for pixel in bytes.chunks_exact_mut(4) {
                    swap_rgba_pa_to_bgra(pixel);
                }
            }

            Ok((bitmap_size, bytes))
        }
    }

    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        // UTF-16 ends of the runs that ask for italics.
        let mut italic_ranges: SmallVec<[(isize, isize); 4]> = SmallVec::new();
        // quiet.md: UTF-16 ranges of the runs, with their font and size (`cjk_fallbacks`).
        let mut sized_ranges: SmallVec<[(isize, isize, FontId, u32, CTFont); 4]> = SmallVec::new();
        // Construct the attributed string, converting UTF8 ranges to UTF16 ranges.
        let mut string = CFMutableAttributedString::new();
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;

        {
            let mut text = text;
            // CoreText merges adjacent runs that end up in equal fonts, including runs of
            // different fonts that fall back to the same one, so every other run that inserts
            // text is set a hair larger to keep ligatures from forming across them. The first
            // run keeps its exact size: SF tracks by its exact point size, and a nudged size
            // draws wider or narrower than AppKit sets the same text.
            let mut break_ligature = false;
            for run in font_runs {
                let text_run;
                (text_run, text) = text.split_at(run.len);

                let utf16_start = string.char_len(); // insert at end of string
                // note: replace_str may silently ignore codepoints it dislikes (e.g., BOM at start of string)
                string.replace_str(&CFString::new(text_run), CFRange::init(utf16_start, 0));
                let utf16_end = string.char_len();

                let length = utf16_end - utf16_start;
                let cf_range = CFRange::init(utf16_start, length);
                if self.is_italic(run.font_id) {
                    italic_ranges.push((utf16_start, utf16_end));
                }
                let font = &self.fonts[run.font_id.0];

                let font_metrics = font.metrics();
                let fonts = &self.fonts;
                let font_scale = f32::from(font_size) / font_metrics.units_per_em as f32;
                max_ascent = max_ascent.max(font_metrics.ascent * font_scale);
                max_descent = max_descent.max(-font_metrics.descent * font_scale);

                let font_size = if break_ligature {
                    px(f32::from(font_size).next_up())
                } else {
                    font_size
                };
                let sized = self
                    .sized
                    .entry((run.font_id, f32::from(font_size).to_bits()))
                    .or_insert_with(|| fonts[run.font_id.0].native_font().clone_with_font_size(font_size.into()))
                    .clone();
                unsafe {
                    string.set_attribute(cf_range, kCTFontAttributeName, &sized);
                }
                let size_bits = f32::from(font_size).to_bits();
                // CJK characters the font has no glyph for get the face
                // CoreText fell back to before (`cjk_fallbacks`).
                if text_run.chars().any(is_cjk_char) && length == text_run.encode_utf16().count() as isize {
                    let mut at = utf16_start;
                    let mut pending: Option<(isize, isize, CTFont)> = None;
                    for ch in text_run.chars() {
                        let n = ch.len_utf16() as isize;
                        let face = if is_cjk_char(ch) { self.cjk_fallbacks.get(&(run.font_id, size_bits, ch)).cloned() } else { None };
                        match (&mut pending, face) {
                            (Some((_, end, f)), Some(face)) if *end == at && f.as_concrete_TypeRef() == face.as_concrete_TypeRef() => *end += n,
                            (_, face) => {
                                if let Some((a, b, f)) = pending.take() {
                                    unsafe { string.set_attribute(CFRange::init(a, b - a), kCTFontAttributeName, &f) };
                                }
                                pending = face.map(|f| (at, at + n, f));
                            }
                        }
                        at += n;
                    }
                    if let Some((a, b, f)) = pending {
                        unsafe { string.set_attribute(CFRange::init(a, b - a), kCTFontAttributeName, &f) };
                    }
                }
                sized_ranges.push((utf16_start, utf16_end, run.font_id, size_bits, sized));
                if length > 0 {
                    break_ligature = !break_ligature;
                }
            }
        }
        // Retrieve the glyphs from the shaped line, converting UTF16 offsets to UTF8 offsets.
        let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
        let glyph_runs = line.glyph_runs();
        let mut runs = <Vec<ShapedRun>>::with_capacity(glyph_runs.len() as usize);
        let mut ix_converter = StringIndexConverter::new(text);
        for run in glyph_runs.into_iter() {
            let attributes = run.attributes().unwrap();
            let font = unsafe {
                attributes
                    .get(kCTFontAttributeName)
                    .downcast::<CTFont>()
                    .unwrap()
            };
            // quiet.md: CoreText fell back from the run's font: keep the face for its CJK characters.
            let first16 = run.string_indices().first().copied().unwrap_or(-1);
            let fell_back = sized_ranges
                .iter()
                .find(|r| (r.0..r.1).contains(&first16))
                .filter(|r| r.4.as_concrete_TypeRef() != font.as_concrete_TypeRef())
                .map(|r| (r.2, r.3));
            let mut font_id = self.id_for_native_font(font.clone());
            // A fallback face for italic text that has no italic of its own.
            let first = run.string_indices().first().copied().unwrap_or(-1);
            if !self.is_italic(font_id)
                && !self.is_emoji(font_id)
                && italic_ranges.iter().any(|&(a, b)| (a..b).contains(&first))
            {
                font_id = self.oblique_twin(font_id);
            }

            let glyphs = match runs.last_mut() {
                Some(run) if run.font_id == font_id => &mut run.glyphs,
                _ => {
                    runs.push(ShapedRun {
                        font_id,
                        glyphs: Vec::with_capacity(run.glyph_count().try_into().unwrap_or(0)),
                    });
                    &mut runs.last_mut().unwrap().glyphs
                }
            };
            for ((&glyph_id, position), &glyph_utf16_ix) in run
                .glyphs()
                .iter()
                .zip(run.positions().iter())
                .zip(run.string_indices().iter())
            {
                let glyph_utf16_ix = usize::try_from(glyph_utf16_ix).unwrap();
                if ix_converter.utf16_ix > glyph_utf16_ix {
                    // We cannot reuse current index converter, as it can only seek forward. Restart the search.
                    ix_converter = StringIndexConverter::new(text);
                }
                ix_converter.advance_to_utf16_ix(glyph_utf16_ix);
                if let Some((from, size_bits)) = fell_back
                    && let Some(ch) = text[ix_converter.utf8_ix..].chars().next()
                    && is_cjk_char(ch)
                {
                    self.cjk_fallbacks.entry((from, size_bits, ch)).or_insert_with(|| font.clone());
                }
                glyphs.push(ShapedGlyph {
                    id: GlyphId(glyph_id as u32),
                    position: point(position.x as f32, position.y as f32).map(px),
                    index: ix_converter.utf8_ix,
                    is_emoji: self.is_emoji(font_id),
                });
            }
        }
        let typographic_bounds = line.get_typographic_bounds();
        LineLayout {
            runs,
            font_size,
            width: typographic_bounds.width.into(),
            ascent: max_ascent.into(),
            descent: max_descent.into(),
            len: text.len(),
        }
    }
}

#[derive(Debug, Clone)]
struct StringIndexConverter<'a> {
    text: &'a str,
    /// Index in UTF-8 bytes
    utf8_ix: usize,
    /// Index in UTF-16 code units
    utf16_ix: usize,
}

impl<'a> StringIndexConverter<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            utf8_ix: 0,
            utf16_ix: 0,
        }
    }

    fn advance_to_utf16_ix(&mut self, utf16_target: usize) {
        for (ix, c) in self.text[self.utf8_ix..].char_indices() {
            if self.utf16_ix >= utf16_target {
                self.utf8_ix += ix;
                return;
            }
            self.utf16_ix += c.len_utf16();
        }
        self.utf8_ix = self.text.len();
    }
}

fn font_kit_metrics_to_metrics(metrics: Metrics) -> FontMetrics {
    FontMetrics {
        units_per_em: metrics.units_per_em,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        underline_position: metrics.underline_position,
        underline_thickness: metrics.underline_thickness,
        cap_height: metrics.cap_height,
        x_height: metrics.x_height,
        bounding_box: bounds_from_rect(metrics.bounding_box),
    }
}

fn bounds_from_rect(rect: RectF) -> Bounds<f32> {
    Bounds {
        origin: point(rect.origin_x(), rect.origin_y()),
        size: size(rect.width(), rect.height()),
    }
}

fn bounds_from_rect_i(rect: RectI) -> Bounds<DevicePixels> {
    Bounds {
        origin: point(DevicePixels(rect.origin_x()), DevicePixels(rect.origin_y())),
        size: size(DevicePixels(rect.width()), DevicePixels(rect.height())),
    }
}

// impl From<Vector2I> for Size<DevicePixels> {
//     fn from(value: Vector2I) -> Self {
//         size(value.x().into(), value.y().into())
//     }
// }

// impl From<RectI> for Bounds<i32> {
//     fn from(rect: RectI) -> Self {
//         Bounds {
//             origin: point(rect.origin_x(), rect.origin_y()),
//             size: size(rect.width(), rect.height()),
//         }
//     }
// }

// impl From<Point<u32>> for Vector2I {
//     fn from(size: Point<u32>) -> Self {
//         Vector2I::new(size.x as i32, size.y as i32)
//     }
// }

fn size_from_vector2f(vec: Vector2F) -> Size<f32> {
    size(vec.x(), vec.y())
}

fn fontkit_weight(value: FontWeight) -> FontkitWeight {
    FontkitWeight(value.0)
}

/// quiet.md: font-kit's `Font::properties` without its unwraps: a missing
/// trait reads as regular (iOS omits some, see load_family).
fn font_properties(font: &FontKitFont) -> font_kit::properties::Properties {
    let ct = font.native_font();
    let traits = ct.all_traits();
    let number = |key| traits.find(key).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_f64());
    let italic = ct.symbolic_traits() & kCTFontItalicTrait != 0 || number(unsafe { kCTFontSlantTrait }).is_some_and(|s| s.abs() > 0.01);
    // Core Text weights at the CSS hundreds, as font-kit maps them.
    const W: [f64; 9] = [-0.7, -0.5, -0.23, 0.0, 0.2, 0.3, 0.4, 0.6, 0.8];
    let w = number(unsafe { kCTFontWeightTrait }).unwrap_or(0.0);
    let weight = match W.iter().position(|&x| w <= x) {
        Some(0) => 100.0,
        Some(i) => 100.0 * i as f64 + 100.0 * (w - W[i - 1]) / (W[i] - W[i - 1]),
        None => 900.0,
    };
    font_kit::properties::Properties {
        style: if italic { FontkitStyle::Italic } else { FontkitStyle::Normal },
        weight: FontkitWeight(weight as f32),
        stretch: Default::default(),
    }
}

fn fontkit_style(style: FontStyle) -> FontkitStyle {
    match style {
        FontStyle::Normal => FontkitStyle::Normal,
        FontStyle::Italic => FontkitStyle::Italic,
        FontStyle::Oblique => FontkitStyle::Oblique,
    }
}

// Some fonts may have no attributes despite `core_text` requiring them (and panicking).
// This is the same version as `core_text` has without `expect` calls.
mod lenient_font_attributes {
    use core_foundation::{
        base::{CFRetain, CFType, TCFType},
        string::{CFString, CFStringRef},
    };
    use core_text::font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCopyAttribute, kCTFontFamilyNameAttribute,
    };

    pub fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
        unsafe { get_string_attribute(descriptor, kCTFontFamilyNameAttribute) }
    }

    fn get_string_attribute(
        descriptor: &CTFontDescriptor,
        attribute: CFStringRef,
    ) -> Option<String> {
        unsafe {
            let value = CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), attribute);
            if value.is_null() {
                return None;
            }

            let value = CFType::wrap_under_create_rule(value);
            assert!(value.instance_of::<CFString>());
            let s = wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef);
            Some(s.to_string())
        }
    }

    unsafe fn wrap_under_get_rule(reference: CFStringRef) -> CFString {
        unsafe {
            assert!(!reference.is_null(), "Attempted to create a NULL object.");
            let reference = CFRetain(reference as *const ::std::os::raw::c_void) as CFStringRef;
            TCFType::wrap_under_create_rule(reference)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::AppleTextSystem;
    use gpui::{
        FontFallbacks, FontFeatures, FontId, FontRun, GlyphId, PlatformTextSystem, font, px,
    };
    use std::sync::Arc;

    #[test]
    fn test_reload_family_with_features_and_fallbacks() {
        let fonts = AppleTextSystem::new();
        let mut font = font("Helvetica");
        let original_id = fonts.font_id(&font).unwrap();
        font.features = FontFeatures::disable_ligatures();
        font.fallbacks = Some(FontFallbacks(Arc::new(vec!["Times".into()])));
        let configured_id = fonts.font_id(&font).unwrap();
        assert_ne!(original_id, configured_id);
        assert_eq!(fonts.font_id(&font).unwrap(), configured_id);
        assert!(fonts.glyph_for_char(configured_id, 'm').is_some());
        let layout = fonts.layout_line(
            "office",
            px(16.),
            &[FontRun {
                font_id: configured_id,
                len: 6,
            }],
        );
        assert_eq!(layout.len, 6);
        assert!(!layout.runs.is_empty());
    }

    #[test]
    fn test_layout_line_sets_a_lone_run_at_its_exact_size() {
        use core_foundation::attributed_string::CFMutableAttributedString;
        use core_foundation::base::{CFRange, TCFType};
        use core_foundation::string::CFString;
        use core_text::line::CTLine;
        use core_text::string_attributes::kCTFontAttributeName;

        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font(".SystemUIFont")).unwrap();
        let text = "Hamburgefonstiv 0123 Wafer";
        let bom = "\u{feff}";
        let with_bom = format!("{bom}{text}");
        for size in [11., 17., 22.] {
            let exact = fonts.0.read().fonts[font_id.0]
                .native_font()
                .clone_with_font_size(size as f64);
            let mut string = CFMutableAttributedString::new();
            string.replace_str(&CFString::new(text), CFRange::init(0, 0));
            let range = CFRange::init(0, string.char_len());
            unsafe { string.set_attribute(range, kCTFontAttributeName, &exact) };
            let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
            let width = line.get_typographic_bounds().width as f32;

            let layout = fonts.layout_line(
                text,
                px(size),
                &[FontRun {
                    font_id,
                    len: text.len(),
                }],
            );
            assert_eq!(f32::from(layout.width), width, "{size} px");

            // replace_str drops a leading BOM, so its run must not count as the previous run.
            let layout = fonts.layout_line(
                &with_bom,
                px(size),
                &[
                    FontRun {
                        font_id,
                        len: bom.len(),
                    },
                    FontRun {
                        font_id,
                        len: text.len(),
                    },
                ],
            );
            assert_eq!(f32::from(layout.width), width, "{size} px after a BOM run");
        }
    }

    #[test]
    fn test_layout_line_breaks_ligatures_between_runs() {
        let fonts = AppleTextSystem::new();
        fonts
            .add_fonts(vec![
                include_bytes!("../../../assets/fonts/lilex/Lilex-Regular.ttf")
                    .as_slice()
                    .into(),
            ])
            .unwrap();
        let glyphs = |text: &str, runs: &[(FontId, usize)]| -> Vec<GlyphId> {
            let runs = runs
                .iter()
                .map(|&(font_id, len)| FontRun { font_id, len })
                .collect::<Vec<_>>();
            fonts
                .layout_line(text, px(16.), &runs)
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
                .collect()
        };

        let lilex = fonts.font_id(&font("Lilex")).unwrap();
        let gt = glyphs(">", &[(lilex, 1)])[0];
        assert_ne!(glyphs(">>>", &[(lilex, 3)]), vec![gt; 3], ">>> as one run");
        assert_eq!(
            glyphs(">>>", &[(lilex, 1), (lilex, 1), (lilex, 1)]),
            vec![gt; 3]
        );
        let mut split = glyphs(">>", &[(lilex, 2)]);
        split.push(gt);
        assert_eq!(glyphs(">>>", &[(lilex, 2), (lilex, 1)]), split);
        let mut split = vec![gt];
        split.extend(glyphs(">>", &[(lilex, 2)]));
        assert_eq!(glyphs(">>>", &[(lilex, 1), (lilex, 2)]), split);

        // Distinct font ids that resolve to the same font.
        let zed_mono = fonts.font_id(&font(".ZedMono")).unwrap();
        let mut no_fallbacks = font("Lilex");
        no_fallbacks.fallbacks = Some(FontFallbacks(Arc::new(Vec::new())));
        let no_fallbacks = fonts.font_id(&no_fallbacks).unwrap();
        for other in [zed_mono, no_fallbacks] {
            assert_ne!(lilex, other);
            assert_eq!(
                glyphs(">>>", &[(lilex, 1), (other, 1), (lilex, 1)]),
                vec![gt; 3]
            );
        }

        // Empty runs between two runs of one font.
        let times = fonts.font_id(&font("Times")).unwrap();
        let helvetica = fonts.font_id(&font("Helvetica")).unwrap();
        let system = fonts.font_id(&font(".SystemUIFont")).unwrap();
        let mut split = glyphs("f", &[(times, 1)]);
        split.extend(glyphs("i", &[(times, 1)]));
        assert_ne!(glyphs("fi", &[(times, 2)]), split, "fi as one run");
        assert_eq!(
            glyphs("fi", &[(times, 1), (helvetica, 0), (system, 0), (times, 1)]),
            split
        );

        // Runs of different fonts that both fall back to Geeza Pro for Arabic.
        assert_eq!(glyphs("لا", &[(helvetica, 4)]).len(), 1, "لا as one run");
        assert_eq!(glyphs("لا", &[(helvetica, 2), (times, 2)]).len(), 2);
    }

    #[test]
    fn test_layout_line_bom_char() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();
        let line = "\u{feff}";
        let mut style = FontRun {
            font_id,
            len: line.len(),
        };

        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert!(layout.runs.is_empty());

        let line = "a\u{feff}b";
        style.len = line.len();
        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b

        let line = "\u{feff}ab";
        let font_runs = &[
            FontRun {
                len: "\u{feff}".len(),
                font_id,
            },
            FontRun {
                len: "ab".len(),
                font_id,
            },
        ];
        let layout = fonts.layout_line(line, px(16.), font_runs);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b
    }

    #[test]
    fn test_layout_line_zwnj_insertion() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello world";
        let font_runs = &[
            FontRun { font_id, len: 5 }, // "hello"
            FontRun { font_id, len: 6 }, // " world"
        ];

        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with different font runs - should not insert ZWNJ
        let font_id2 = fonts.font_id(&font("Times")).unwrap_or(font_id);
        let font_runs_different = &[
            FontRun { font_id, len: 5 }, // "hello"
            // " world"
            FontRun {
                font_id: font_id2,
                len: 6,
            },
        ];

        let layout2 = fonts.layout_line(text, px(16.), font_runs_different);
        assert_eq!(layout2.len, text.len());

        for run in &layout2.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }
    }

    #[test]
    fn test_layout_line_zwnj_edge_cases() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello";
        let font_runs = &[FontRun { font_id, len: 5 }];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        let text = "abc";
        let font_runs = &[
            FontRun { font_id, len: 1 }, // "a"
            FontRun { font_id, len: 1 }, // "b"
            FontRun { font_id, len: 1 }, // "c"
        ];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with empty text
        let text = "";
        let font_runs = &[];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, 0);
        assert!(layout.runs.is_empty());
    }
}
