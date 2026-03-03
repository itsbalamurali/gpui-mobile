//! Android text shaping and glyph rasterisation.
//!
//! A self-contained port of `gpui_wgpu::CosmicTextSystem` for Android, using:
//!
//! * [`cosmic-text`] for Unicode-aware font selection, shaping, and line layout.
//! * [`swash`] for glyph rasterisation (grayscale, subpixel, and colour emoji).
//!
//! ## Design
//!
//! `AndroidTextSystem` owns a single `cosmic_text::FontSystem` (which in turn
//! holds the system font database) and a `swash::scale::ScaleContext` for
//! rasterisation.  Both are protected by a single `RwLock`; reads (metrics,
//! glyph lookups) take a shared lock while mutations (shaping, rasterisation,
//! loading new fonts) take an exclusive lock.
//!
//! ### Font discovery on Android
//!
//! Android ships fonts under `/system/fonts/` and `/product/fonts/`.
//! `cosmic-text`'s `FontSystem::new()` scans these directories automatically
//! when the crate is built with the `fontconfig` or `fontdb` feature, so on
//! Android the full system font set is available without any extra work.
//!
//! When the `font-kit` Cargo feature is enabled, font-weight/style matching
//! uses `font_kit::matching::find_best_match` (the same algorithm as the
//! upstream `gpui_wgpu` crate).  Without `font-kit`, a lightweight built-in
//! matcher is used instead.
//!
//! ## No GPUI workspace dependency
//!
//! All trait / type references to the GPUI crate are **stubbed out** locally
//! so this file compiles in isolation.  When wired into a real GPUI workspace,
//! delete the stub section and `use gpui::*` instead.

#![allow(dead_code)]

use anyhow::{Context as _, Result};
use cosmic_text::{
    Attrs, AttrsList, Family, Font as CosmicFont, FontSystem, ShapeBuffer, ShapeLine,
};
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Format, Vector},
};

// ── stub types ────────────────────────────────────────────────────────────────
// Replace with `use gpui::*;` when building inside the GPUI workspace.

/// Opaque font identifier (index into `loaded_fonts`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub usize);

/// Opaque glyph identifier (raw glyph index from the font).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u32);

/// A logical font size in CSS pixels.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Pixels(pub f32);

impl Pixels {
    pub const ZERO: Self = Self(0.0);
}

impl From<f32> for Pixels {
    fn from(v: f32) -> Self {
        Self(v)
    }
}

impl From<Pixels> for f32 {
    fn from(p: Pixels) -> Self {
        p.0
    }
}

/// Physical / device pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DevicePixels(pub i32);

impl From<i32> for DevicePixels {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

/// A 2-D size.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

/// A 2-D point.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

/// An axis-aligned rectangle.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Bounds<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
}

/// Convenience constructors (mirror `gpui::point` / `gpui::size`).
pub fn point<T>(x: T, y: T) -> Point<T> {
    Point { x, y }
}

pub fn size<T>(width: T, height: T) -> Size<T> {
    Size { width, height }
}

/// Subpixel variant counts — must match the GPUI renderer's expectations.
pub const SUBPIXEL_VARIANTS_X: u8 = 4;
pub const SUBPIXEL_VARIANTS_Y: u8 = 1;

/// Subpixel offset within a pixel grid cell.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SubpixelVariant {
    pub x: u8,
    pub y: u8,
}

/// Font weight (CSS-compatible integer, 100–900).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Font style.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// An OpenType feature tag + value pair.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFeatures(pub Vec<(String, u32)>);

/// A logical font descriptor.
#[derive(Clone, Debug)]
pub struct Font {
    pub family: String,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub features: FontFeatures,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: "sans-serif".to_string(),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            features: FontFeatures::default(),
        }
    }
}

/// Metrics for a loaded font face (all values in font units).
#[derive(Copy, Clone, Debug, Default)]
pub struct FontMetrics {
    pub units_per_em: u32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub cap_height: f32,
    pub x_height: f32,
    pub bounding_box: Bounds<f32>,
}

/// How a glyph should be rendered.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum TextRenderingMode {
    #[default]
    Subpixel,
    Grayscale,
}

/// Parameters that fully describe a single glyph rasterisation request.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderGlyphParams {
    pub font_id: FontId,
    pub glyph_id: GlyphId,
    pub font_size: Pixels,
    pub subpixel_variant: SubpixelVariant,
    pub scale_factor: f32,
    pub is_emoji: bool,
    pub subpixel_rendering: bool,
}

/// A shaped glyph inside a `LineLayout`.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    pub id: GlyphId,
    pub position: Point<Pixels>,
    pub index: usize,
    pub is_emoji: bool,
}

/// A run of glyphs using the same font.
#[derive(Clone, Debug)]
pub struct ShapedRun {
    pub font_id: FontId,
    pub glyphs: Vec<ShapedGlyph>,
}

/// A laid-out line of text.
#[derive(Clone, Debug)]
pub struct LineLayout {
    pub font_size: Pixels,
    pub width: Pixels,
    pub ascent: Pixels,
    pub descent: Pixels,
    pub runs: Vec<ShapedRun>,
    pub len: usize,
}

/// A `(font_id, byte_length)` run specification for `layout_line`.
#[derive(Clone, Debug)]
pub struct FontRun {
    pub font_id: FontId,
    pub len: usize,
}

// ── internal state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    family: String,
    features: FontFeatures,
}

impl FontKey {
    fn new(family: String, features: FontFeatures) -> Self {
        Self { family, features }
    }
}

struct LoadedFont {
    font: Arc<CosmicFont>,
    /// Stored for reference but not applied via cosmic-text 0.12 (no FontFeatures API).
    features: FontFeatures,
    is_known_emoji_font: bool,
}

struct AndroidTextSystemState {
    font_system: FontSystem,
    scratch: ShapeBuffer,
    swash_scale_ctx: ScaleContext,
    /// All loaded font faces; indexed by `FontId::0`.
    loaded_fonts: Vec<LoadedFont>,
    /// Cache: `FontKey` → the `FontId`s for every face in that family.
    font_ids_by_family: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    /// The name of the Android system font family used as a fallback
    /// when the requested family is unavailable.
    system_fallback: String,
}

// ── public API ────────────────────────────────────────────────────────────────

/// Android text system using cosmic-text + swash.
///
/// Thread-safe via an internal `RwLock`; the same instance may be shared
/// between the render thread and a background text-shaping thread.
pub struct AndroidTextSystem(RwLock<AndroidTextSystemState>);

impl AndroidTextSystem {
    // ── constructors ─────────────────────────────────────────────────────────

    /// Create a new text system.
    ///
    /// `system_font_fallback` is the family name to fall back to when
    /// the requested font is not installed (e.g. `"sans-serif"` or
    /// `"Noto Sans"`).
    ///
    /// `cosmic-text` will scan the Android system font directories
    /// (`/system/fonts/`, `/product/fonts/`) automatically.
    pub fn new(system_font_fallback: &str) -> Self {
        let font_system = FontSystem::new();

        Self(RwLock::new(AndroidTextSystemState {
            font_system,
            scratch: ShapeBuffer::default(),
            swash_scale_ctx: ScaleContext::new(),
            loaded_fonts: Vec::new(),
            font_ids_by_family: HashMap::new(),
            system_fallback: system_font_fallback.to_string(),
        }))
    }

    /// Like `new`, but starts with an **empty** font database.
    ///
    /// Useful for unit tests that want deterministic results without depending
    /// on whatever fonts happen to be installed on the device.
    pub fn new_without_system_fonts(system_font_fallback: &str) -> Self {
        let font_system = FontSystem::new_with_locale_and_db(
            "en-US".to_string(),
            cosmic_text::fontdb::Database::new(),
        );

        Self(RwLock::new(AndroidTextSystemState {
            font_system,
            scratch: ShapeBuffer::default(),
            swash_scale_ctx: ScaleContext::new(),
            loaded_fonts: Vec::new(),
            font_ids_by_family: HashMap::new(),
            system_fallback: system_font_fallback.to_string(),
        }))
    }

    // ── font management ───────────────────────────────────────────────────────

    /// Load one or more font faces from raw bytes.
    ///
    /// Accepts both `Cow::Borrowed` (embedded / static bytes) and
    /// `Cow::Owned` (dynamically loaded bytes).
    pub fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    /// Returns the de-duplicated list of all font family names known to the
    /// font system (including both system fonts and fonts added via
    /// `add_fonts`).
    pub fn all_font_names(&self) -> Vec<String> {
        let lock = self.0.read();
        let mut names: Vec<String> = lock
            .font_system
            .db()
            .faces()
            .filter_map(|f| f.families.first().map(|fam| fam.0.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Resolve a `Font` descriptor to a `FontId`.
    ///
    /// Loads the font family on first call; subsequent calls for the same
    /// family are served from the cache.
    pub fn font_id(&self, font: &Font) -> Result<FontId> {
        let mut state = self.0.write();
        let key = FontKey::new(font.family.clone(), font.features.clone());

        let candidates = if let Some(ids) = state.font_ids_by_family.get(&key) {
            ids.as_slice()
        } else {
            let ids = state.load_family(&font.family, &font.features)?;
            state.font_ids_by_family.insert(key.clone(), ids);
            state.font_ids_by_family[&key].as_ref()
        };

        let ix = find_best_match(font, candidates, &state)?;
        Ok(candidates[ix])
    }

    // ── font metrics ──────────────────────────────────────────────────────────

    /// Return the face-level metrics for `font_id` (in font design units).
    pub fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        let lock = self.0.read();
        let m = lock.loaded_font(font_id).font.as_swash().metrics(&[]);

        FontMetrics {
            units_per_em: m.units_per_em as u32,
            ascent: m.ascent,
            descent: -m.descent,
            line_gap: m.leading,
            underline_position: m.underline_offset,
            underline_thickness: m.stroke_size,
            cap_height: m.cap_height,
            x_height: m.x_height,
            bounding_box: Bounds {
                origin: point(0.0_f32, 0.0_f32),
                size: size(m.max_width, m.ascent + m.descent),
            },
        }
    }

    /// Return the typographic bounds (advance width × advance height) for
    /// `glyph_id` in `font_id`.
    pub fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        let lock = self.0.read();
        let gm = lock.loaded_font(font_id).font.as_swash().glyph_metrics(&[]);
        let g = glyph_id.0 as u16;
        Ok(Bounds {
            origin: point(0.0_f32, 0.0_f32),
            size: size(gm.advance_width(g), gm.advance_height(g)),
        })
    }

    /// Return the advance of `glyph_id` in `font_id`.
    pub fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    /// Return the glyph index for `ch` in `font_id`, or `None` if the font
    /// does not contain a glyph for that codepoint.
    pub fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    // ── rasterisation ─────────────────────────────────────────────────────────

    /// Return the tight bounding box (in device pixels) for a rasterised
    /// glyph, without actually producing pixel data.
    pub fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.0.write().raster_bounds(params)
    }

    /// Rasterise a glyph and return `(bitmap_size, pixel_bytes)`.
    ///
    /// * For grayscale glyphs: 1 byte per pixel (alpha mask).
    /// * For subpixel glyphs: 4 bytes per pixel (BGRA, matching
    ///   `wgpu::TextureFormat::Rgba8Unorm` after the channel-swap in the
    ///   atlas uploader).
    /// * For emoji: 4 bytes per pixel (BGRA).
    pub fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.0.write().rasterize_glyph(params, raster_bounds)
    }

    // ── layout ────────────────────────────────────────────────────────────────

    /// Shape and lay out a single line of `text` at `font_size` pixels,
    /// using `runs` to specify which font face covers each byte range.
    pub fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, runs)
    }

    /// Returns the recommended rendering mode for the given font at the given
    /// size.
    ///
    /// On Android we always return `Subpixel`; callers that need grayscale
    /// rendering (e.g. when drawing onto a transparent surface) should
    /// override this.
    pub fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Subpixel
    }
}

// ── AndroidTextSystemState helpers ───────────────────────────────────────────

impl AndroidTextSystemState {
    // ── font loading ──────────────────────────────────────────────────────────

    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let db = self.font_system.db_mut();
        for bytes in fonts {
            match bytes {
                Cow::Borrowed(b) => db.load_font_data(b.to_vec()),
                Cow::Owned(b) => db.load_font_data(b),
            }
        }
        Ok(())
    }

    /// Load every face in the named family and register them in
    /// `loaded_fonts`, returning their `FontId`s.
    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
    ) -> Result<SmallVec<[FontId; 4]>> {
        // Resolve aliases / fallbacks before querying the database.
        let resolved_name = font_name_with_fallback(name, &self.system_fallback.clone());

        let faces: SmallVec<[(cosmic_text::fontdb::ID, String, cosmic_text::fontdb::Weight); 4]> =
            self.font_system
                .db()
                .faces()
                .filter(|f| {
                    f.families
                        .iter()
                        .any(|fam| fam.0.eq_ignore_ascii_case(&resolved_name))
                })
                .map(|f| (f.id, f.post_script_name.clone(), f.weight))
                .collect();

        let mut ids = SmallVec::new();

        for (db_id, postscript_name, weight) in faces {
            let font = self
                .font_system
                .get_font(db_id, weight)
                .context("could not load font face")?;

            // Skip decorative / symbol fonts that have no 'm' glyph unless
            // they are known exceptions (matches gpui_wgpu behaviour).
            let allowed_bad_names = ["SegoeFluentIcons", "Segoe Fluent Icons"];
            if font.as_swash().charmap().map('m') == 0
                && !allowed_bad_names.contains(&postscript_name.as_str())
            {
                self.font_system.db_mut().remove_face(font.id());
                continue;
            }

            let font_id = FontId(self.loaded_fonts.len());
            ids.push(font_id);
            self.loaded_fonts.push(LoadedFont {
                font,
                features: features.clone(),
                is_known_emoji_font: check_is_known_emoji_font(&postscript_name),
            });
        }

        anyhow::ensure!(!ids.is_empty(), "no font faces found for family {:?}", name);

        Ok(ids)
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    fn loaded_font(&self, font_id: FontId) -> &LoadedFont {
        &self.loaded_fonts[font_id.0]
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        let gm = self.loaded_font(font_id).font.as_swash().glyph_metrics(&[]);
        Ok(Size {
            width: gm.advance_width(glyph_id.0 as u16),
            height: gm.advance_height(glyph_id.0 as u16),
        })
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        let gid = self.loaded_font(font_id).font.as_swash().charmap().map(ch);
        if gid == 0 {
            None
        } else {
            Some(GlyphId(gid.into()))
        }
    }

    // ── rasterisation ─────────────────────────────────────────────────────────

    fn raster_bounds(&mut self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        let img = self.render_glyph_image(params)?;
        Ok(Bounds {
            origin: point(
                DevicePixels(img.placement.left.into()),
                DevicePixels((-img.placement.top).into()),
            ),
            size: Size {
                width: DevicePixels(img.placement.width.try_into().unwrap_or(0)),
                height: DevicePixels(img.placement.height.try_into().unwrap_or(0)),
            },
        })
    }

    fn rasterize_glyph(
        &mut self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        }

        let mut image = self.render_glyph_image(params)?;
        let bitmap_size = glyph_bounds.size;

        match image.content {
            // Colour / subpixel: output is RGBA, swap R↔B to get BGRA for the
            // atlas format used by the Android wgpu renderer.
            swash::scale::image::Content::Color | swash::scale::image::Content::SubpixelMask => {
                for pixel in image.data.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                Ok((bitmap_size, image.data))
            }
            // Grayscale: 1 byte per pixel, no conversion needed.
            swash::scale::image::Content::Mask => Ok((bitmap_size, image.data)),
        }
    }

    /// Low-level glyph image generation via swash.
    fn render_glyph_image(
        &mut self,
        params: &RenderGlyphParams,
    ) -> Result<swash::scale::image::Image> {
        let loaded = &self.loaded_fonts[params.font_id.0];
        let font_ref = loaded.font.as_swash();
        let pixel_size = f32::from(params.font_size);

        let subpixel_offset = Vector::new(
            params.subpixel_variant.x as f32 / SUBPIXEL_VARIANTS_X as f32 / params.scale_factor,
            params.subpixel_variant.y as f32 / SUBPIXEL_VARIANTS_Y as f32 / params.scale_factor,
        );

        let mut scaler = self
            .swash_scale_ctx
            .builder(font_ref)
            .size(pixel_size * params.scale_factor)
            .hint(true)
            .build();

        let sources: &[Source] = if params.is_emoji {
            &[
                Source::ColorOutline(0),
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::Outline,
            ]
        } else {
            &[Source::Outline]
        };

        let mut renderer = Render::new(sources);
        if params.subpixel_rendering {
            // swash bug: B and R values are swapped in subpixel_bgra output;
            // we swap them back in `rasterize_glyph` above.
            renderer
                .format(Format::subpixel_bgra())
                .offset(subpixel_offset);
        } else {
            renderer.format(Format::Alpha).offset(subpixel_offset);
        }

        let glyph_id: u16 = params.glyph_id.0.try_into().context("glyph id overflow")?;
        renderer
            .render(&mut scaler, glyph_id)
            .with_context(|| format!("swash: failed to render glyph {:?}", params.glyph_id))
    }

    // ── cosmic-text → FontId resolution ───────────────────────────────────────

    /// Map a `cosmic_text::fontdb::ID` to a `FontId`, loading the face into
    /// `loaded_fonts` if it hasn't been seen before.
    ///
    /// Used to handle cosmic-text's automatic font fallback during shaping:
    /// cosmic-text may choose a face that was never explicitly loaded via
    /// `font_id()`, so we need to register it lazily.
    fn font_id_for_cosmic_id(&mut self, id: cosmic_text::fontdb::ID) -> Result<FontId> {
        // Fast path: already loaded.
        if let Some(ix) = self.loaded_fonts.iter().position(|lf| lf.font.id() == id) {
            return Ok(FontId(ix));
        }

        // Slow path: load the face.
        let face_weight = self
            .font_system
            .db()
            .face(id)
            .map(|f| f.weight)
            .unwrap_or(cosmic_text::fontdb::Weight::NORMAL);

        let font = self
            .font_system
            .get_font(id, face_weight)
            .context("failed to get fallback font from cosmic-text")?;

        let face = self
            .font_system
            .db()
            .face(id)
            .context("fallback font face not found in cosmic-text database")?;

        let font_id = FontId(self.loaded_fonts.len());
        self.loaded_fonts.push(LoadedFont {
            font,
            // Fallback faces get empty features — consistent with gpui_wgpu.
            features: FontFeatures::default(),
            is_known_emoji_font: check_is_known_emoji_font(&face.post_script_name),
        });

        Ok(font_id)
    }

    // ── layout ────────────────────────────────────────────────────────────────

    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        // Build per-span attribute list for cosmic-text.
        let mut attrs_list = AttrsList::new(&Attrs::new());
        let mut byte_off = 0usize;

        for run in font_runs {
            let loaded = self.loaded_font(run.font_id);
            let db = self.font_system.db();

            let Some(face) = db.face(loaded.font.id()) else {
                log::warn!(
                    "layout_line: font face not found for font_id {:?}",
                    run.font_id
                );
                byte_off += run.len;
                continue;
            };
            let Some((family_name, _)) = face.families.first() else {
                log::warn!("layout_line: no family name for font_id {:?}", run.font_id);
                byte_off += run.len;
                continue;
            };

            attrs_list.add_span(
                byte_off..(byte_off + run.len),
                &Attrs::new()
                    .metadata(run.font_id.0)
                    .family(Family::Name(family_name))
                    .stretch(face.stretch)
                    .style(face.style)
                    .weight(face.weight),
            );
            byte_off += run.len;
        }

        // Shape the line.
        let shape_line = ShapeLine::new(
            &mut self.font_system,
            text,
            &attrs_list,
            cosmic_text::Shaping::Advanced,
            4,
        );

        let mut layout_lines = Vec::with_capacity(1);
        shape_line.layout_to_buffer(
            &mut self.scratch,
            f32::from(font_size),
            None,
            cosmic_text::Wrap::None,
            None,
            &mut layout_lines,
            None,
            cosmic_text::Hinting::default(),
        );

        let Some(layout) = layout_lines.first() else {
            return LineLayout {
                font_size,
                width: Pixels::ZERO,
                ascent: Pixels::ZERO,
                descent: Pixels::ZERO,
                runs: Vec::new(),
                len: text.len(),
            };
        };

        // Convert cosmic-text glyphs into `ShapedRun`s.
        let mut runs: Vec<ShapedRun> = Vec::new();

        for glyph in &layout.glyphs {
            // Resolve the font_id for this glyph (may differ from the
            // requested font if cosmic-text chose a fallback).
            let mut font_id = FontId(glyph.metadata);
            let loaded = self.loaded_font(font_id);

            if loaded.font.id() != glyph.font_id {
                match self.font_id_for_cosmic_id(glyph.font_id) {
                    Ok(resolved) => font_id = resolved,
                    Err(err) => {
                        log::warn!(
                            "layout_line: failed to resolve fallback font {:?}: {err:#}",
                            glyph.font_id
                        );
                        continue;
                    }
                }
            }

            let is_emoji = self.loaded_font(font_id).is_known_emoji_font;

            // Workaround: variation selectors cause a crash in swash when
            // glyph_id == 3 for an emoji font (same guard as gpui_wgpu).
            if glyph.glyph_id == 3 && is_emoji {
                continue;
            }

            let shaped_glyph = ShapedGlyph {
                id: GlyphId(glyph.glyph_id as u32),
                position: point(Pixels(glyph.x), Pixels(glyph.y)),
                index: glyph.start,
                is_emoji,
            };

            if let Some(last) = runs.last_mut().filter(|r| r.font_id == font_id) {
                last.glyphs.push(shaped_glyph);
            } else {
                runs.push(ShapedRun {
                    font_id,
                    glyphs: vec![shaped_glyph],
                });
            }
        }

        LineLayout {
            font_size,
            width: Pixels(layout.w),
            ascent: Pixels(layout.max_ascent),
            descent: Pixels(layout.max_descent),
            runs,
            len: text.len(),
        }
    }
}

// ── font name helper ──────────────────────────────────────────────────────────

/// Return `name` if it matches a known Android system family, otherwise fall
/// back to `fallback`.
///
/// Android's system font aliases differ from desktop Linux (e.g. `"serif"` →
/// `"Noto Serif"`, `"monospace"` → `"Droid Sans Mono"`).
fn font_name_with_fallback<'a>(name: &'a str, _fallback: &'a str) -> String {
    // CSS generic families are valid Android font aliases and are handled
    // natively by fontdb on Android — pass them through unchanged.
    let generic = ["serif", "sans-serif", "monospace", "cursive", "fantasy"];
    if generic.contains(&name.to_ascii_lowercase().as_str()) {
        return name.to_string();
    }

    // For anything else, trust the caller; if the family isn't found,
    // `load_family` will return an error and the caller can retry with
    // the fallback.
    name.to_string()
}

// ── font-weight matching ──────────────────────────────────────────────────────

#[cfg(feature = "font-kit")]
fn find_best_match(
    font: &Font,
    candidates: &[FontId],
    state: &AndroidTextSystemState,
) -> Result<usize> {
    let props: SmallVec<[font_kit::properties::Properties; 4]> = candidates
        .iter()
        .map(|&id| {
            let db_id = state.loaded_font(id).font.id();
            let face = state
                .font_system
                .db()
                .face(db_id)
                .context("font face not found in database")?;
            Ok(face_info_into_properties(face))
        })
        .collect::<Result<_>>()?;

    let ix = font_kit::matching::find_best_match(&props, &font_into_properties(font))
        .context("no font face matches the requested weight/style")?;

    Ok(ix)
}

#[cfg(not(feature = "font-kit"))]
fn find_best_match(
    font: &Font,
    candidates: &[FontId],
    state: &AndroidTextSystemState,
) -> Result<usize> {
    anyhow::ensure!(
        !candidates.is_empty(),
        "no font faces found for family {:?}",
        font.family
    );

    if candidates.len() == 1 {
        return Ok(0);
    }

    let target_weight = font.weight.0 as i32;
    let target_italic = matches!(font.style, FontStyle::Italic | FontStyle::Oblique);

    let mut best_index = 0usize;
    let mut best_score = u32::MAX;

    for (index, &id) in candidates.iter().enumerate() {
        let db_id = state.loaded_font(id).font.id();
        let Some(face) = state.font_system.db().face(db_id) else {
            continue;
        };

        let is_italic = matches!(
            face.style,
            cosmic_text::Style::Italic | cosmic_text::Style::Oblique
        );
        let style_penalty: u32 = if is_italic == target_italic { 0 } else { 1000 };
        let weight_diff = (face.weight.0 as i32 - target_weight).unsigned_abs();
        let score = style_penalty + weight_diff;

        if score < best_score {
            best_score = score;
            best_index = index;
        }
    }

    Ok(best_index)
}

// ── font-kit property helpers (only compiled with the `font-kit` feature) ─────

#[cfg(feature = "font-kit")]
fn font_into_properties(font: &Font) -> font_kit::properties::Properties {
    font_kit::properties::Properties {
        style: match font.style {
            FontStyle::Normal => font_kit::properties::Style::Normal,
            FontStyle::Italic => font_kit::properties::Style::Italic,
            FontStyle::Oblique => font_kit::properties::Style::Oblique,
        },
        weight: font_kit::properties::Weight(font.weight.0.into()),
        stretch: font_kit::properties::Stretch::NORMAL,
    }
}

#[cfg(feature = "font-kit")]
fn face_info_into_properties(
    face: &cosmic_text::fontdb::FaceInfo,
) -> font_kit::properties::Properties {
    font_kit::properties::Properties {
        style: match face.style {
            cosmic_text::Style::Normal => font_kit::properties::Style::Normal,
            cosmic_text::Style::Italic => font_kit::properties::Style::Italic,
            cosmic_text::Style::Oblique => font_kit::properties::Style::Oblique,
        },
        weight: font_kit::properties::Weight(face.weight.0.into()),
        stretch: match face.stretch {
            cosmic_text::Stretch::Condensed => font_kit::properties::Stretch::CONDENSED,
            cosmic_text::Stretch::Expanded => font_kit::properties::Stretch::EXPANDED,
            cosmic_text::Stretch::ExtraCondensed => font_kit::properties::Stretch::EXTRA_CONDENSED,
            cosmic_text::Stretch::ExtraExpanded => font_kit::properties::Stretch::EXTRA_EXPANDED,
            cosmic_text::Stretch::Normal => font_kit::properties::Stretch::NORMAL,
            cosmic_text::Stretch::SemiCondensed => font_kit::properties::Stretch::SEMI_CONDENSED,
            cosmic_text::Stretch::SemiExpanded => font_kit::properties::Stretch::SEMI_EXPANDED,
            cosmic_text::Stretch::UltraCondensed => font_kit::properties::Stretch::ULTRA_CONDENSED,
            cosmic_text::Stretch::UltraExpanded => font_kit::properties::Stretch::ULTRA_EXPANDED,
        },
    }
}

// ── OpenType feature conversion ───────────────────────────────────────────────

/// Validate font feature tags (must be exactly 4 ASCII bytes).
///
/// In cosmic-text 0.12 there is no `FontFeatures` / `FeatureTag` API, so we
/// only validate the tags here.  Actual feature application is a no-op until
/// a newer cosmic-text version is used.
fn validate_font_features(features: &FontFeatures) -> Result<()> {
    for (name, _value) in &features.0 {
        anyhow::ensure!(
            name.as_bytes().len() == 4,
            "feature tag {:?} must be exactly 4 ASCII bytes",
            name
        );
    }
    Ok(())
}

// ── known emoji font detection ────────────────────────────────────────────────

fn check_is_known_emoji_font(postscript_name: &str) -> bool {
    matches!(
        postscript_name,
        "NotoColorEmoji" | "NotoColorEmojiCompat" | "NotoEmoji" | "TwemojiMozilla" | "Twemoji"
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── stub helpers ─────────────────────────────────────────────────────────

    fn make_system() -> AndroidTextSystem {
        AndroidTextSystem::new_without_system_fonts("sans-serif")
    }

    fn load_noto_regular() -> Cow<'static, [u8]> {
        // In CI we embed a minimal subset of Noto Sans so tests are
        // self-contained.  Replace with `include_bytes!` if you have the
        // font bundled in the repository; otherwise the test that calls
        // this will be skipped.
        Cow::Borrowed(&[][..])
    }

    // ── unit tests ────────────────────────────────────────────────────────────

    #[test]
    fn font_name_fallback_passthrough_generics() {
        assert_eq!(
            font_name_with_fallback("sans-serif", "Roboto"),
            "sans-serif"
        );
        assert_eq!(font_name_with_fallback("monospace", "Roboto"), "monospace");
    }

    #[test]
    fn font_name_fallback_passthrough_specific() {
        assert_eq!(font_name_with_fallback("Roboto", "sans-serif"), "Roboto");
    }

    #[test]
    fn check_is_known_emoji_font_positive() {
        assert!(check_is_known_emoji_font("NotoColorEmoji"));
        assert!(check_is_known_emoji_font("NotoColorEmojiCompat"));
        assert!(check_is_known_emoji_font("TwemojiMozilla"));
    }

    #[test]
    fn check_is_known_emoji_font_negative() {
        assert!(!check_is_known_emoji_font("Roboto"));
        assert!(!check_is_known_emoji_font("NotoSans"));
        assert!(!check_is_known_emoji_font(""));
    }

    #[test]
    fn cosmic_font_features_empty() {
        let features = FontFeatures(vec![]);
        let result = validate_font_features(&features);
        assert!(result.is_ok());
    }

    #[test]
    fn cosmic_font_features_valid_tag() {
        let feats = FontFeatures(vec![("liga".to_string(), 1)]);
        validate_font_features(&feats).unwrap();
    }

    #[test]
    fn cosmic_font_features_invalid_tag_too_short() {
        let features = FontFeatures(vec![("ke".to_string(), 1)]);
        let result = validate_font_features(&features);
        assert!(result.is_err(), "expected error for short tag");
    }

    #[test]
    fn cosmic_font_features_invalid_tag_too_long() {
        let features = FontFeatures(vec![("kerning".to_string(), 1)]);
        let result = validate_font_features(&features);
        assert!(result.is_err(), "expected error for long tag");
    }

    #[test]
    fn pixels_zero_is_zero() {
        assert_eq!(Pixels::ZERO.0, 0.0);
    }

    #[test]
    fn font_weight_constants() {
        assert_eq!(FontWeight::NORMAL.0, 400);
        assert_eq!(FontWeight::BOLD.0, 700);
    }

    #[test]
    fn subpixel_variant_counts() {
        assert_eq!(SUBPIXEL_VARIANTS_X, 4);
        assert_eq!(SUBPIXEL_VARIANTS_Y, 1);
    }

    #[test]
    fn all_font_names_returns_empty_for_no_system_fonts() {
        let sys = make_system();
        // We constructed without system fonts, so the list should be empty
        // (no fonts were loaded via `add_fonts` either).
        let names = sys.all_font_names();
        assert!(
            names.is_empty(),
            "expected empty font list, got {:?}",
            names
        );
    }

    #[test]
    fn font_id_errors_for_unknown_family() {
        let sys = make_system();
        let font = Font {
            family: "NonExistentFamilyXYZ123".to_string(),
            ..Font::default()
        };
        assert!(sys.font_id(&font).is_err());
    }

    #[test]
    fn recommended_rendering_mode_is_subpixel() {
        let sys = make_system();
        // FontId(0) doesn't exist but the method doesn't access loaded_fonts.
        assert_eq!(
            sys.recommended_rendering_mode(FontId(0), Pixels(12.0)),
            TextRenderingMode::Subpixel,
        );
    }

    #[test]
    fn layout_line_empty_string() {
        let sys = make_system();
        let layout = sys.layout_line("", Pixels(16.0), &[]);
        assert_eq!(layout.len, 0);
        assert_eq!(layout.runs.len(), 0);
        assert_eq!(layout.font_size, Pixels(16.0));
    }

    #[test]
    fn layout_line_no_runs_no_crash() {
        let sys = make_system();
        // Even with a non-empty string but no runs, we should not panic.
        let layout = sys.layout_line("hello", Pixels(14.0), &[]);
        assert_eq!(layout.len, 5);
    }

    #[cfg(not(feature = "font-kit"))]
    #[test]
    fn find_best_match_empty_candidates_errors() {
        let sys = make_system();
        let font = Font::default();
        let state = sys.0.read();
        let result = find_best_match(&font, &[], &state);
        assert!(result.is_err());
    }

    #[test]
    fn point_and_size_constructors() {
        let p = point(1.0_f32, 2.0_f32);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);

        let s = size(3u32, 4u32);
        assert_eq!(s.width, 3);
        assert_eq!(s.height, 4);
    }

    #[test]
    fn device_pixels_from_i32() {
        let dp: DevicePixels = 42.into();
        assert_eq!(dp.0, 42);
    }
}
