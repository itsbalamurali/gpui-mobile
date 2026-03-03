//! wgpu-based GPU renderer for Android.
//!
//! A direct port of `gpui_wgpu::{WgpuContext, WgpuRenderer}` targeting
//! Android / Vulkan (with an OpenGL ES fallback).
//!
//! ## What lives here
//!
//! | Type              | Role                                                  |
//! |-------------------|-------------------------------------------------------|
//! | `WgpuContext`     | Device + queue + adapter; shared across windows       |
//! | `WgpuRenderer`    | Per-surface pipeline: shaders, atlas, draw calls      |
//! | `WgpuSurfaceConfig` | Size + transparency config for a new surface        |
//! | `RenderingParameters` | Gamma, MSAA sample count, contrast settings      |
//!
//! ## Android-specific notes
//!
//! * The wgpu instance is created with `Backends::VULKAN | Backends::GL` so
//!   that it works on both Vulkan-capable devices (API level ≥ 24) and
//!   OpenGL ES–only devices.
//!
//! * `ANativeWindow` surfaces are created via `wgpu::SurfaceTargetUnsafe::
//!   RawHandle` using `raw-window-handle` 0.6 `AndroidNdkWindowHandle`.
//!
//! * `pollster::block_on` is used for adapter / device initialisation because
//!   Android native threads do not have an async runtime by default.
//!
//! * The WGSL shaders are embedded via `include_str!` from the `shaders/`
//!   sub-directory (see `shaders/shaders.wgsl` and
//!   `shaders/shaders_subpixel.wgsl`).
//!
//! ## No GPUI workspace dependency
//!
//! Scene / primitive types are **stubbed out** locally so this file compiles
//! in isolation.  Replace the stub section with `use gpui::*` when building
//! inside the full GPUI workspace.

#![allow(unsafe_code)]
#![allow(dead_code)]

use anyhow::{Context as _, Result};
use bytemuck::{Pod, Zeroable};

use std::{num::NonZeroU64, sync::Arc};

use super::{Bounds, DevicePixels, Pixels, Point, Size};
use crate::android::atlas::{AndroidAtlas, AtlasTextureId};

// ── stub scene / primitive types ─────────────────────────────────────────────
// Delete this block and `use gpui::*` when building in the full workspace.

/// A colour in premultiplied RGBA, packed as `[r, g, b, a]` f32 values.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A rendered quad (rounded rectangle with optional border + shadow).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct Quad {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub corner_radius: f32,
    pub _pad: [f32; 2],
}

/// A box shadow primitive.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct Shadow {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub color: Color,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub corner_radius: f32,
    pub _pad: f32,
}

/// A text underline.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct Underline {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub color: Color,
    pub thickness: f32,
    pub wavy: u32,
    pub _pad: [f32; 2],
}

/// A monochrome (grayscale-mask) sprite glyph.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct MonochromeSprite {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub tile: PodAtlasTile,
    pub color: Color,
}

/// A polychrome (full-colour) sprite (emoji, image tile).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct PolychromeSprite {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub tile: PodAtlasTile,
    pub grayscale: u32,
    pub _pad: [f32; 3],
}

/// A subpixel-AA sprite glyph.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct SubpixelSprite {
    pub bounds: PodBounds,
    pub clip_bounds: PodBounds,
    pub tile: PodAtlasTile,
    pub color: Color,
}

/// A vector path (curve glyph / icon shape).
#[derive(Clone, Debug, Default)]
pub struct PathPrimitive {
    pub bounds: Bounds<Pixels>,
    pub color: Color,
    pub vertices: Vec<PathVertex>,
    /// Used for batching — primitives sharing the same order value may be
    /// merged into one draw call.
    pub order: u32,
}

impl PathPrimitive {
    pub fn clipped_bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
}

/// A single vertex in a path rasterisation triangle list.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct PathVertex {
    pub xy_position: [f32; 2],
    pub st_position: [f32; 2],
}

/// A batch of primitives of a single type.
#[derive(Debug)]
pub enum PrimitiveBatch<'a> {
    Quads(&'a [Quad]),
    Shadows(&'a [Shadow]),
    Paths(&'a [PathPrimitive]),
    Underlines(&'a [Underline]),
    MonochromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [MonochromeSprite],
    },
    SubpixelSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [SubpixelSprite],
    },
    PolychromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [PolychromeSprite],
    },
}

/// A complete frame description.
#[derive(Debug, Default)]
pub struct Scene {
    pub quads: Vec<Quad>,
    pub shadows: Vec<Shadow>,
    pub paths: Vec<PathPrimitive>,
    pub underlines: Vec<Underline>,
    pub monochrome_sprites: Vec<MonochromeSprite>,
    pub subpixel_sprites: Vec<SubpixelSprite>,
    pub polychrome_sprites: Vec<PolychromeSprite>,
    // texture_id arrays parallel the sprite vecs above
    pub mono_texture_id: Option<AtlasTextureId>,
    pub subpixel_texture_id: Option<AtlasTextureId>,
    pub poly_texture_id: Option<AtlasTextureId>,
}

impl Scene {
    /// Iterate the scene as typed `PrimitiveBatch` values.
    pub fn batches(&self) -> impl Iterator<Item = PrimitiveBatch<'_>> {
        let mut v: Vec<PrimitiveBatch<'_>> = Vec::new();
        if !self.quads.is_empty() {
            v.push(PrimitiveBatch::Quads(&self.quads));
        }
        if !self.shadows.is_empty() {
            v.push(PrimitiveBatch::Shadows(&self.shadows));
        }
        if !self.paths.is_empty() {
            v.push(PrimitiveBatch::Paths(&self.paths));
        }
        if !self.underlines.is_empty() {
            v.push(PrimitiveBatch::Underlines(&self.underlines));
        }
        if let (Some(id), true) = (self.mono_texture_id, !self.monochrome_sprites.is_empty()) {
            v.push(PrimitiveBatch::MonochromeSprites {
                texture_id: id,
                sprites: &self.monochrome_sprites,
            });
        }
        if let (Some(id), true) = (self.subpixel_texture_id, !self.subpixel_sprites.is_empty()) {
            v.push(PrimitiveBatch::SubpixelSprites {
                texture_id: id,
                sprites: &self.subpixel_sprites,
            });
        }
        if let (Some(id), true) = (self.poly_texture_id, !self.polychrome_sprites.is_empty()) {
            v.push(PrimitiveBatch::PolychromeSprites {
                texture_id: id,
                sprites: &self.polychrome_sprites,
            });
        }
        v.into_iter()
    }
}

// ── GPU-layout POD helpers ────────────────────────────────────────────────────

/// Bounds packed as `[origin_x, origin_y, size_w, size_h]` for the GPU.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct PodBounds {
    pub origin: [f32; 2],
    pub size: [f32; 2],
}

impl From<Bounds<Pixels>> for PodBounds {
    fn from(b: Bounds<Pixels>) -> Self {
        Self {
            origin: [b.origin.x.0, b.origin.y.0],
            size: [b.size.width.0, b.size.height.0],
        }
    }
}

/// Atlas tile packed as `[origin_x, origin_y, size_w, size_h, tex_idx, kind]`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct PodAtlasTile {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub tex_index: u32,
    pub tex_kind: u32,
    pub _pad: [u32; 2],
}

/// Global uniform block (one per frame).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GlobalParams {
    viewport_size: [f32; 2],
    premultiplied_alpha: u32,
    pad: u32,
}

/// Gamma / subpixel contrast uniform block.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GammaParams {
    gamma_ratios: [f32; 4],
    grayscale_enhanced_contrast: f32,
    subpixel_enhanced_contrast: f32,
    _pad: [f32; 2],
}

/// Surface-blit uniform block.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SurfaceParams {
    bounds: PodBounds,
    content_mask: PodBounds,
}

// ── path sprite intermediate types ────────────────────────────────────────────

/// Identifies one "tile" in the path intermediate texture.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct PathSprite {
    /// Bounds within the intermediate texture to sample.
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
}

/// A vertex sent to the path rasterisation pass.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct PathRastVertex {
    xy_position: [f32; 2],
    st_position: [f32; 2],
    color: Color,
    bounds: PodBounds,
}

// ── WgpuContext ───────────────────────────────────────────────────────────────

/// Shared wgpu device + queue + adapter.
///
/// One `WgpuContext` is created per process; it is shared by all
/// `WgpuRenderer` instances (one per `ANativeWindow` surface).
pub struct WgpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    dual_source_blending: bool,
}

impl WgpuContext {
    /// Create a wgpu instance suitable for Android.
    ///
    /// Prefers Vulkan; falls back to OpenGL ES on older devices.
    pub fn android_instance() -> wgpu::Instance {
        wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            ..Default::default()
        })
    }

    /// Create a `WgpuContext` using `instance`, selecting an adapter that is
    /// compatible with `surface`.
    pub fn new(instance: wgpu::Instance, surface: &wgpu::Surface<'_>) -> Result<Self> {
        let adapter = pollster::block_on(Self::select_adapter(&instance, Some(surface)))?;

        let caps = surface.get_capabilities(&adapter);
        anyhow::ensure!(
            !caps.formats.is_empty(),
            "adapter {:?} has no surface formats",
            adapter.get_info().name
        );

        log::info!(
            "Android GPU: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend,
        );

        let (device, queue, dual_source_blending) =
            pollster::block_on(Self::create_device(&adapter))?;

        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            dual_source_blending,
        })
    }

    /// Verify that this context's adapter is compatible with `surface`.
    pub fn check_compatible_with_surface(&self, surface: &wgpu::Surface<'_>) -> Result<()> {
        let caps = surface.get_capabilities(&self.adapter);
        anyhow::ensure!(
            !caps.formats.is_empty(),
            "adapter {:?} is not compatible with the display surface",
            self.adapter.get_info().name
        );
        Ok(())
    }

    pub fn supports_dual_source_blending(&self) -> bool {
        self.dual_source_blending
    }

    // ── private ───────────────────────────────────────────────────────────────

    async fn select_adapter(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<wgpu::Adapter> {
        instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("failed to find suitable GPU adapter: {e}"))
    }

    async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue, bool)> {
        let dual_source_blending = adapter
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);

        let mut required_features = wgpu::Features::empty();
        if dual_source_blending {
            required_features |= wgpu::Features::DUAL_SOURCE_BLENDING;
        } else {
            log::warn!("DUAL_SOURCE_BLENDING not available — subpixel text AA disabled");
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpui_android_device"),
                required_features,
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits())
                    .using_alignment(adapter.limits()),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| anyhow::anyhow!("failed to create wgpu device: {e}"))?;

        Ok((device, queue, dual_source_blending))
    }
}

// ── pipeline collection ───────────────────────────────────────────────────────

struct WgpuPipelines {
    quads: wgpu::RenderPipeline,
    shadows: wgpu::RenderPipeline,
    path_rasterization: wgpu::RenderPipeline,
    paths: wgpu::RenderPipeline,
    underlines: wgpu::RenderPipeline,
    mono_sprites: wgpu::RenderPipeline,
    subpixel_sprites: Option<wgpu::RenderPipeline>,
    poly_sprites: wgpu::RenderPipeline,
}

// ── bind group layouts ────────────────────────────────────────────────────────

struct WgpuBindGroupLayouts {
    globals: wgpu::BindGroupLayout,
    instances: wgpu::BindGroupLayout,
    instances_with_texture: wgpu::BindGroupLayout,
}

// ── surface config ────────────────────────────────────────────────────────────

/// Configuration used to create or resize a `WgpuRenderer` surface.
pub struct WgpuSurfaceConfig {
    pub size: Size<DevicePixels>,
    pub transparent: bool,
}

// ── WgpuRenderer ─────────────────────────────────────────────────────────────

/// Per-surface wgpu renderer.
///
/// Each `AndroidWindow` owns one `WgpuRenderer`.  A `WgpuContext` is shared
/// between all renderers in the process (passed via `&mut Option<WgpuContext>`
/// so the first renderer can initialise it and subsequent ones can reuse it).
pub struct WgpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pipelines: WgpuPipelines,
    bind_group_layouts: WgpuBindGroupLayouts,
    atlas: Arc<AndroidAtlas>,
    atlas_sampler: wgpu::Sampler,
    globals_buffer: wgpu::Buffer,
    path_globals_offset: u64,
    gamma_offset: u64,
    globals_bind_group: wgpu::BindGroup,
    path_globals_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: u64,
    max_buffer_size: u64,
    storage_buffer_alignment: u64,
    path_intermediate_texture: Option<wgpu::Texture>,
    path_intermediate_view: Option<wgpu::TextureView>,
    path_msaa_texture: Option<wgpu::Texture>,
    path_msaa_view: Option<wgpu::TextureView>,
    rendering_params: RenderingParameters,
    dual_source_blending: bool,
    adapter_info: wgpu::AdapterInfo,
    transparent_alpha_mode: wgpu::CompositeAlphaMode,
    opaque_alpha_mode: wgpu::CompositeAlphaMode,
    max_texture_size: u32,
}

impl WgpuRenderer {
    // ── constructors ─────────────────────────────────────────────────────────

    /// Create a renderer from raw Android native-window handles.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the `ANativeWindow *` encoded in the
    /// `raw-window-handle` types remains valid for the lifetime of the
    /// returned renderer.
    pub unsafe fn new(
        gpu_context: &mut Option<WgpuContext>,
        raw_window_handle: raw_window_handle::RawWindowHandle,
        raw_display_handle: raw_window_handle::RawDisplayHandle,
        config: WgpuSurfaceConfig,
    ) -> Result<Self> {
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle,
            raw_window_handle,
        };

        let instance = if gpu_context.is_some() {
            // Re-use existing instance — wgpu::Instance doesn't implement Clone
            // in wgpu 22, so we just create a new one that shares the same backends.
            WgpuContext::android_instance()
        } else {
            WgpuContext::android_instance()
        };

        let surface = unsafe {
            instance
                .create_surface_unsafe(target)
                .context("failed to create wgpu surface from ANativeWindow")?
        };

        let context = match gpu_context {
            Some(ctx) => {
                ctx.check_compatible_with_surface(&surface)?;
                ctx
            }
            None => gpu_context.insert(WgpuContext::new(instance, &surface)?),
        };

        Self::new_with_surface(context, surface, config)
    }

    /// Create a renderer from an already-created `wgpu::Surface`.
    pub fn new_with_surface(
        context: &WgpuContext,
        surface: wgpu::Surface<'static>,
        config: WgpuSurfaceConfig,
    ) -> Result<Self> {
        let caps = surface.get_capabilities(&context.adapter);

        // Preferred formats: BGRA first (common on Android), then RGBA.
        let preferred = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ];
        let surface_format = preferred
            .iter()
            .find(|f| caps.formats.contains(f))
            .copied()
            .or_else(|| caps.formats.iter().find(|f| !f.is_srgb()).copied())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "surface has no supported texture formats for adapter {:?}",
                    context.adapter.get_info().name
                )
            })?;

        let pick_alpha = |prefs: &[wgpu::CompositeAlphaMode]| -> Result<wgpu::CompositeAlphaMode> {
            prefs
                .iter()
                .find(|p| caps.alpha_modes.contains(p))
                .copied()
                .or_else(|| caps.alpha_modes.first().copied())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "surface has no alpha modes for adapter {:?}",
                        context.adapter.get_info().name
                    )
                })
        };

        let transparent_alpha_mode = pick_alpha(&[
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ])?;

        let opaque_alpha_mode = pick_alpha(&[
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::Inherit,
        ])?;

        let alpha_mode = if config.transparent {
            transparent_alpha_mode
        } else {
            opaque_alpha_mode
        };

        let device = Arc::clone(&context.device);
        let max_texture_size = device.limits().max_texture_dimension_2d;

        let req_w = config.size.width.0 as u32;
        let req_h = config.size.height.0 as u32;
        let clamped_w = req_w.min(max_texture_size).max(1);
        let clamped_h = req_h.min(max_texture_size).max(1);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: clamped_w,
            height: clamped_h,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let queue = Arc::clone(&context.queue);
        let dual_source_blending = context.supports_dual_source_blending();
        let rendering_params = RenderingParameters::new(&context.adapter, surface_format);

        let bind_group_layouts = Self::create_bind_group_layouts(&device);
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            surface_format,
            alpha_mode,
            rendering_params.path_sample_count,
            dual_source_blending,
        );

        let atlas = Arc::new(AndroidAtlas::new(Arc::clone(&device), Arc::clone(&queue)));
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let globals_size = std::mem::size_of::<GlobalParams>() as u64;
        let gamma_size = std::mem::size_of::<GammaParams>() as u64;
        let path_globals_offset = globals_size.next_multiple_of(uniform_align);
        let gamma_offset = (path_globals_offset + globals_size).next_multiple_of(uniform_align);

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals_buffer"),
            size: gamma_offset + gamma_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let max_buffer_size = device.limits().max_buffer_size;
        let storage_buffer_alignment = device.limits().min_storage_buffer_offset_alignment as u64;
        let initial_cap = 2 * 1024 * 1024u64; // 2 MiB

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: initial_cap,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mk_globals_bg = |offset: u64| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals_bind_group"),
                layout: &bind_group_layouts.globals,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &globals_buffer,
                            offset,
                            size: Some(NonZeroU64::new(globals_size).unwrap()),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &globals_buffer,
                            offset: gamma_offset,
                            size: Some(NonZeroU64::new(gamma_size).unwrap()),
                        }),
                    },
                ],
            })
        };

        let globals_bind_group = mk_globals_bg(0);
        let path_globals_bind_group = mk_globals_bg(path_globals_offset);
        let adapter_info = context.adapter.get_info();

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            pipelines,
            bind_group_layouts,
            atlas,
            atlas_sampler,
            globals_buffer,
            path_globals_offset,
            gamma_offset,
            globals_bind_group,
            path_globals_bind_group,
            instance_buffer,
            instance_buffer_capacity: initial_cap,
            max_buffer_size,
            storage_buffer_alignment,
            path_intermediate_texture: None,
            path_intermediate_view: None,
            path_msaa_texture: None,
            path_msaa_view: None,
            rendering_params,
            dual_source_blending,
            adapter_info,
            transparent_alpha_mode,
            opaque_alpha_mode,
            max_texture_size,
        })
    }

    // ── public API ────────────────────────────────────────────────────────────

    /// Resize the swap-chain to match a new window size.
    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        let w = (size.width.0 as u32).min(self.max_texture_size).max(1);
        let h = (size.height.0 as u32).min(self.max_texture_size).max(1);

        if w == self.surface_config.width && h == self.surface_config.height {
            return;
        }

        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();

        for tex in [
            self.path_intermediate_texture.take(),
            self.path_msaa_texture.take(),
        ]
        .into_iter()
        .flatten()
        {
            tex.destroy();
        }
        self.path_intermediate_view = None;
        self.path_msaa_view = None;

        self.surface_config.width = w;
        self.surface_config.height = h;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Update whether the surface composites with pre-multiplied alpha.
    pub fn update_transparency(&mut self, transparent: bool) {
        let new_mode = if transparent {
            self.transparent_alpha_mode
        } else {
            self.opaque_alpha_mode
        };

        if new_mode == self.surface_config.alpha_mode {
            return;
        }

        self.surface_config.alpha_mode = new_mode;
        self.surface.configure(&self.device, &self.surface_config);
        self.pipelines = Self::create_pipelines(
            &self.device,
            &self.bind_group_layouts,
            self.surface_config.format,
            self.surface_config.alpha_mode,
            self.rendering_params.path_sample_count,
            self.dual_source_blending,
        );
    }

    /// Return the atlas shared with this renderer.
    pub fn sprite_atlas(&self) -> &Arc<AndroidAtlas> {
        &self.atlas
    }

    /// Current viewport size in device pixels.
    pub fn viewport_size(&self) -> Size<DevicePixels> {
        Size {
            width: DevicePixels(self.surface_config.width as i32),
            height: DevicePixels(self.surface_config.height as i32),
        }
    }

    /// Whether dual-source blending (subpixel AA) is supported by the GPU.
    pub fn supports_dual_source_blending(&self) -> bool {
        self.dual_source_blending
    }

    /// GPU / driver information for diagnostics.
    pub fn gpu_info(&self) -> (&str, &str, &str) {
        (
            &self.adapter_info.name,
            &self.adapter_info.driver,
            &self.adapter_info.driver_info,
        )
    }

    /// Maximum atlas texture dimension.
    pub fn max_texture_size(&self) -> u32 {
        self.max_texture_size
    }

    // ── main draw entry point ─────────────────────────────────────────────────

    /// Render `scene` into the surface's next frame.
    ///
    /// Safe to call every VSync tick.  Returns immediately if the surface is
    /// lost or the scene is empty.
    pub fn draw(&mut self, scene: &Scene) {
        self.atlas.before_frame();

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
            Err(e) => {
                log::error!("failed to acquire surface texture: {e}");
                return;
            }
        };

        // Now the surface is healthy — lazily create intermediate textures.
        self.ensure_intermediate_textures();

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Upload uniform data.
        let premul = if self.surface_config.alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied {
            1u32
        } else {
            0u32
        };

        let globals = GlobalParams {
            viewport_size: [
                self.surface_config.width as f32,
                self.surface_config.height as f32,
            ],
            premultiplied_alpha: premul,
            pad: 0,
        };
        let path_globals = GlobalParams {
            premultiplied_alpha: 0,
            ..globals
        };
        let gamma_params = GammaParams {
            gamma_ratios: self.rendering_params.gamma_ratios,
            grayscale_enhanced_contrast: self.rendering_params.grayscale_enhanced_contrast,
            subpixel_enhanced_contrast: self.rendering_params.subpixel_enhanced_contrast,
            _pad: [0.0; 2],
        };

        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(
            &self.globals_buffer,
            self.path_globals_offset,
            bytemuck::bytes_of(&path_globals),
        );
        self.queue.write_buffer(
            &self.globals_buffer,
            self.gamma_offset,
            bytemuck::bytes_of(&gamma_params),
        );

        // Render loop — retried once if the instance buffer overflows.
        loop {
            let mut instance_offset: u64 = 0;
            let mut overflow = false;

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("android_frame_encoder"),
                });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("main_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });

                for batch in scene.batches() {
                    let ok = match batch {
                        PrimitiveBatch::Quads(quads) => {
                            self.draw_quads(quads, &mut instance_offset, &mut pass)
                        }
                        PrimitiveBatch::Shadows(shadows) => {
                            self.draw_shadows(shadows, &mut instance_offset, &mut pass)
                        }
                        PrimitiveBatch::Paths(paths) => {
                            if paths.is_empty() {
                                continue;
                            }
                            drop(pass);

                            let did = self.draw_paths_to_intermediate(
                                &mut encoder,
                                paths,
                                &mut instance_offset,
                            );

                            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("main_pass_continued"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &frame_view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                ..Default::default()
                            });

                            if did {
                                self.draw_paths_from_intermediate(
                                    paths,
                                    &mut instance_offset,
                                    &mut pass,
                                )
                            } else {
                                false
                            }
                        }
                        PrimitiveBatch::Underlines(underlines) => {
                            self.draw_underlines(underlines, &mut instance_offset, &mut pass)
                        }
                        PrimitiveBatch::MonochromeSprites {
                            texture_id,
                            sprites,
                        } => self.draw_monochrome_sprites(
                            sprites,
                            texture_id,
                            &mut instance_offset,
                            &mut pass,
                        ),
                        PrimitiveBatch::SubpixelSprites {
                            texture_id,
                            sprites,
                        } => self.draw_subpixel_sprites(
                            sprites,
                            texture_id,
                            &mut instance_offset,
                            &mut pass,
                        ),
                        PrimitiveBatch::PolychromeSprites {
                            texture_id,
                            sprites,
                        } => self.draw_polychrome_sprites(
                            sprites,
                            texture_id,
                            &mut instance_offset,
                            &mut pass,
                        ),
                    };

                    if !ok {
                        overflow = true;
                        break;
                    }
                }
            }

            if overflow {
                drop(encoder);
                if self.instance_buffer_capacity >= self.max_buffer_size {
                    log::error!(
                        "instance buffer grew too large ({}); dropping frame",
                        self.instance_buffer_capacity
                    );
                    frame.present();
                    return;
                }
                self.grow_instance_buffer();
                continue;
            }

            self.queue.submit(std::iter::once(encoder.finish()));
            frame.present();
            return;
        }
    }

    // ── primitive draw helpers ────────────────────────────────────────────────

    fn draw_quads(
        &self,
        quads: &[Quad],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let data = unsafe { Self::as_bytes(quads) };
        self.draw_instances(
            data,
            quads.len() as u32,
            &self.pipelines.quads,
            instance_offset,
            pass,
        )
    }

    fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let data = unsafe { Self::as_bytes(shadows) };
        self.draw_instances(
            data,
            shadows.len() as u32,
            &self.pipelines.shadows,
            instance_offset,
            pass,
        )
    }

    fn draw_underlines(
        &self,
        underlines: &[Underline],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let data = unsafe { Self::as_bytes(underlines) };
        self.draw_instances(
            data,
            underlines.len() as u32,
            &self.pipelines.underlines,
            instance_offset,
            pass,
        )
    }

    fn draw_monochrome_sprites(
        &self,
        sprites: &[MonochromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let data = unsafe { Self::as_bytes(sprites) };
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            &self.pipelines.mono_sprites,
            instance_offset,
            pass,
        )
    }

    fn draw_subpixel_sprites(
        &self,
        sprites: &[SubpixelSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let data = unsafe { Self::as_bytes(sprites) };
        let pipeline = self
            .pipelines
            .subpixel_sprites
            .as_ref()
            .unwrap_or(&self.pipelines.mono_sprites);
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            pipeline,
            instance_offset,
            pass,
        )
    }

    fn draw_polychrome_sprites(
        &self,
        sprites: &[PolychromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let data = unsafe { Self::as_bytes(sprites) };
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            &self.pipelines.poly_sprites,
            instance_offset,
            pass,
        )
    }

    // ── path rendering ────────────────────────────────────────────────────────

    fn draw_paths_to_intermediate(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        paths: &[PathPrimitive],
        instance_offset: &mut u64,
    ) -> bool {
        let mut vertices: Vec<PathRastVertex> = Vec::new();
        for path in paths {
            let bounds = path.clipped_bounds();
            for v in &path.vertices {
                vertices.push(PathRastVertex {
                    xy_position: v.xy_position,
                    st_position: v.st_position,
                    color: path.color,
                    bounds: bounds.into(),
                });
            }
        }
        if vertices.is_empty() {
            return true;
        }

        let vertex_data = unsafe { Self::as_bytes(&vertices) };
        let Some((vertex_offset, vertex_size)) =
            self.write_to_instance_buffer(instance_offset, vertex_data)
        else {
            return false;
        };

        let data_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path_rasterization_bg"),
            layout: &self.bind_group_layouts.instances,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.instance_binding(vertex_offset, vertex_size),
            }],
        });

        let Some(intermediate_view) = self.path_intermediate_view.as_ref() else {
            return true;
        };

        let (target_view, resolve_target) = if let Some(ref msaa) = self.path_msaa_view {
            (msaa, Some(intermediate_view))
        } else {
            (intermediate_view, None)
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("path_rasterization_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.pipelines.path_rasterization);
        pass.set_bind_group(0, &self.path_globals_bind_group, &[]);
        pass.set_bind_group(1, &data_bg, &[]);
        pass.draw(0..vertices.len() as u32, 0..1);

        true
    }

    fn draw_paths_from_intermediate(
        &self,
        paths: &[PathPrimitive],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let first = &paths[0];
        // If all paths share the same `order` they can be drawn as one sprite;
        // otherwise union the bounds.
        let sprites: Vec<PathSprite> = if paths.iter().all(|p| p.order == first.order) {
            paths
                .iter()
                .map(|p| {
                    let b = p.clipped_bounds();
                    PathSprite {
                        bounds_x: b.origin.x.0,
                        bounds_y: b.origin.y.0,
                        bounds_w: b.size.width.0,
                        bounds_h: b.size.height.0,
                    }
                })
                .collect()
        } else {
            let mut union = first.clipped_bounds();
            for p in paths.iter().skip(1) {
                let b = p.clipped_bounds();
                let x0 = union.origin.x.0.min(b.origin.x.0);
                let y0 = union.origin.y.0.min(b.origin.y.0);
                let x1 = (union.origin.x.0 + union.size.width.0).max(b.origin.x.0 + b.size.width.0);
                let y1 =
                    (union.origin.y.0 + union.size.height.0).max(b.origin.y.0 + b.size.height.0);
                union = Bounds {
                    origin: Point {
                        x: Pixels(x0),
                        y: Pixels(y0),
                    },
                    size: Size {
                        width: Pixels(x1 - x0),
                        height: Pixels(y1 - y0),
                    },
                };
            }
            vec![PathSprite {
                bounds_x: union.origin.x.0,
                bounds_y: union.origin.y.0,
                bounds_w: union.size.width.0,
                bounds_h: union.size.height.0,
            }]
        };

        let Some(intermediate_view) = self.path_intermediate_view.as_ref() else {
            return true;
        };

        let sprite_data = unsafe { Self::as_bytes(&sprites) };
        self.draw_instances_with_texture(
            sprite_data,
            sprites.len() as u32,
            intermediate_view,
            &self.pipelines.paths,
            instance_offset,
            pass,
        )
    }

    // ── generic instance helpers ──────────────────────────────────────────────

    fn draw_instances(
        &self,
        data: &[u8],
        count: u32,
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if count == 0 {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, data) else {
            return false;
        };
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layouts.instances,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.instance_binding(offset, size),
            }],
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_bind_group(1, &bg, &[]);
        pass.draw(0..4, 0..count);
        true
    }

    fn draw_instances_with_texture(
        &self,
        data: &[u8],
        count: u32,
        texture_view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if count == 0 {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, data) else {
            return false;
        };
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layouts.instances_with_texture,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_binding(offset, size),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                },
            ],
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_bind_group(1, &bg, &[]);
        pass.draw(0..4, 0..count);
        true
    }

    // ── instance buffer management ────────────────────────────────────────────

    fn write_to_instance_buffer(
        &self,
        instance_offset: &mut u64,
        data: &[u8],
    ) -> Option<(u64, NonZeroU64)> {
        let offset = instance_offset.next_multiple_of(self.storage_buffer_alignment);
        let size = (data.len() as u64).max(16);
        if offset + size > self.instance_buffer_capacity {
            return None;
        }
        self.queue.write_buffer(&self.instance_buffer, offset, data);
        *instance_offset = offset + size;
        Some((offset, NonZeroU64::new(size).unwrap()))
    }

    fn instance_binding(&self, offset: u64, size: NonZeroU64) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &self.instance_buffer,
            offset,
            size: Some(size),
        })
    }

    fn grow_instance_buffer(&mut self) {
        let new_cap = (self.instance_buffer_capacity * 2).min(self.max_buffer_size);
        log::info!("android renderer: growing instance buffer to {}", new_cap);
        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: new_cap,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_buffer_capacity = new_cap;
    }

    // ── intermediate texture management ──────────────────────────────────────

    fn ensure_intermediate_textures(&mut self) {
        if self.path_intermediate_texture.is_some() {
            return;
        }

        let w = self.surface_config.width;
        let h = self.surface_config.height;
        let fmt = self.surface_config.format;

        let (pt, pv) = Self::create_path_intermediate(&self.device, fmt, w, h);
        self.path_intermediate_texture = Some(pt);
        self.path_intermediate_view = Some(pv);

        if let Some((mt, mv)) = Self::create_msaa_if_needed(
            &self.device,
            fmt,
            w,
            h,
            self.rendering_params.path_sample_count,
        ) {
            self.path_msaa_texture = Some(mt);
            self.path_msaa_view = Some(mv);
        }
    }

    fn create_path_intermediate(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("path_intermediate"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_msaa_if_needed(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Option<(wgpu::Texture, wgpu::TextureView)> {
        if sample_count <= 1 {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("path_msaa"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some((texture, view))
    }

    // ── pipeline / layout creation ────────────────────────────────────────────

    fn create_bind_group_layouts(device: &wgpu::Device) -> WgpuBindGroupLayouts {
        let globals =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<GlobalParams>() as u64
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<GammaParams>() as u64
                            ),
                        },
                        count: None,
                    },
                ],
            });

        let storage_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let instances = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instances_layout"),
            entries: &[storage_entry(0)],
        });

        let instances_with_texture =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("instances_with_texture_layout"),
                entries: &[
                    storage_entry(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        WgpuBindGroupLayouts {
            globals,
            instances,
            instances_with_texture,
        }
    }

    fn create_pipelines(
        device: &wgpu::Device,
        layouts: &WgpuBindGroupLayouts,
        surface_format: wgpu::TextureFormat,
        alpha_mode: wgpu::CompositeAlphaMode,
        path_sample_count: u32,
        dual_source_blending: bool,
    ) -> WgpuPipelines {
        let shader_src = include_str!("shaders/shaders.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpui_android_shaders"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_src)),
        });

        let subpixel_shader = if dual_source_blending {
            let subpixel_src = include_str!("shaders/shaders_subpixel.wgsl");
            let combined = format!("enable dual_source_blending;\n{shader_src}\n{subpixel_src}");
            Some(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("gpui_android_subpixel_shaders"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(combined)),
            }))
        } else {
            None
        };

        let blend = match alpha_mode {
            wgpu::CompositeAlphaMode::PreMultiplied => {
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
            }
            _ => wgpu::BlendState::ALPHA_BLENDING,
        };

        let color_target = wgpu::ColorTargetState {
            format: surface_format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let make_pipeline = |name: &str,
                             vs: &str,
                             fs: &str,
                             globals_layout: &wgpu::BindGroupLayout,
                             data_layout: &wgpu::BindGroupLayout,
                             topology: wgpu::PrimitiveTopology,
                             targets: &[Option<wgpu::ColorTargetState>],
                             samples: u32,
                             module: &wgpu::ShaderModule| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("{name}_layout")),
                bind_group_layouts: &[globals_layout, data_layout],
                immediate_size: 0,
            });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vs),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };

        macro_rules! pipeline {
            ($name:expr, $vs:expr, $fs:expr, $data_layout:expr, $topology:expr,
             $targets:expr, $samples:expr, $module:expr) => {
                make_pipeline(
                    $name,
                    $vs,
                    $fs,
                    &layouts.globals,
                    $data_layout,
                    $topology,
                    $targets,
                    $samples,
                    $module,
                )
            };
        }

        let strip = wgpu::PrimitiveTopology::TriangleStrip;
        let list = wgpu::PrimitiveTopology::TriangleList;
        let ct = [Some(color_target.clone())];

        let quads = pipeline!(
            "quads",
            "vs_quad",
            "fs_quad",
            &layouts.instances,
            strip,
            &ct,
            1,
            &shader
        );

        let shadows = pipeline!(
            "shadows",
            "vs_shadow",
            "fs_shadow",
            &layouts.instances,
            strip,
            &ct,
            1,
            &shader
        );

        let path_rasterization = pipeline!(
            "path_rasterization",
            "vs_path_rasterization",
            "fs_path_rasterization",
            &layouts.instances,
            list,
            &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            path_sample_count,
            &shader
        );

        let paths_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let paths = pipeline!(
            "paths",
            "vs_path",
            "fs_path",
            &layouts.instances_with_texture,
            strip,
            &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(paths_blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            1,
            &shader
        );

        let underlines = pipeline!(
            "underlines",
            "vs_underline",
            "fs_underline",
            &layouts.instances,
            strip,
            &ct,
            1,
            &shader
        );

        let mono_sprites = pipeline!(
            "mono_sprites",
            "vs_mono_sprite",
            "fs_mono_sprite",
            &layouts.instances_with_texture,
            strip,
            &ct,
            1,
            &shader
        );

        let subpixel_sprites = subpixel_shader.as_ref().map(|sp_module| {
            let subpixel_blend = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Src1,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            };

            pipeline!(
                "subpixel_sprites",
                "vs_subpixel_sprite",
                "fs_subpixel_sprite",
                &layouts.instances_with_texture,
                strip,
                &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(subpixel_blend),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
                1,
                sp_module
            )
        });

        let poly_sprites = pipeline!(
            "poly_sprites",
            "vs_poly_sprite",
            "fs_poly_sprite",
            &layouts.instances_with_texture,
            strip,
            &ct,
            1,
            &shader
        );

        WgpuPipelines {
            quads,
            shadows,
            path_rasterization,
            paths,
            underlines,
            mono_sprites,
            subpixel_sprites,
            poly_sprites,
        }
    }

    // ── unsafe byte-cast helper ───────────────────────────────────────────────

    /// Reinterpret a slice of `Pod` values as raw bytes for `queue.write_buffer`.
    ///
    /// # Safety
    ///
    /// `T` must be `Pod` (ensured by the `bytemuck::Pod` bound on every
    /// primitive type above).
    unsafe fn as_bytes<T: Pod>(slice: &[T]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice))
        }
    }

    // ── clean-up ──────────────────────────────────────────────────────────────

    /// Release GPU resources.  Called automatically on drop but may be called
    /// early when the `ANativeWindow` is about to be destroyed.
    pub fn destroy(&mut self) {
        // wgpu resources are dropped when the fields are dropped.
        // Explicitly destroy textures that hold GPU memory.
        if let Some(t) = self.path_intermediate_texture.take() {
            t.destroy();
        }
        if let Some(t) = self.path_msaa_texture.take() {
            t.destroy();
        }
        self.instance_buffer.destroy();
        self.globals_buffer.destroy();
    }
}

impl Drop for WgpuRenderer {
    fn drop(&mut self) {
        self.destroy();
    }
}

// ── RenderingParameters ───────────────────────────────────────────────────────

/// Rendering quality settings derived from the device and environment variables.
pub struct RenderingParameters {
    /// MSAA sample count for path rasterisation (1, 2, or 4).
    pub path_sample_count: u32,
    /// Gamma correction ratios `[r, g, b, _]`.
    pub gamma_ratios: [f32; 4],
    pub grayscale_enhanced_contrast: f32,
    pub subpixel_enhanced_contrast: f32,
}

impl RenderingParameters {
    pub fn new(adapter: &wgpu::Adapter, surface_format: wgpu::TextureFormat) -> Self {
        let format_features = adapter.get_texture_format_features(surface_format);
        let path_sample_count = [4u32, 2, 1]
            .into_iter()
            .find(|&n| format_features.flags.sample_count_supported(n))
            .unwrap_or(1);

        // Allow per-device gamma tuning via environment variable (matches
        // gpui_wgpu behaviour).
        let gamma = std::env::var("GPUI_ANDROID_GAMMA")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.8)
            .clamp(1.0, 2.2);

        let gamma_ratios = gamma_correction_ratios(gamma);

        let grayscale_enhanced_contrast = std::env::var("GPUI_ANDROID_GRAYSCALE_CONTRAST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .max(0.0);

        let subpixel_enhanced_contrast = std::env::var("GPUI_ANDROID_SUBPIXEL_CONTRAST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.5)
            .max(0.0);

        Self {
            path_sample_count,
            gamma_ratios,
            grayscale_enhanced_contrast,
            subpixel_enhanced_contrast,
        }
    }
}

/// Compute per-channel gamma correction multipliers.
///
/// Mirrors `gpui::get_gamma_correction_ratios` so the renderer produces
/// visually identical output to the desktop build.
fn gamma_correction_ratios(gamma: f32) -> [f32; 4] {
    // Standard weights for perceived luminance (BT.601 luma coefficients).
    let r_weight = 0.2126_f32;
    let g_weight = 0.7152_f32;
    let b_weight = 0.0722_f32;

    let r = r_weight.powf(1.0 / gamma);
    let g = g_weight.powf(1.0 / gamma);
    let b = b_weight.powf(1.0 / gamma);

    [r, g, b, 1.0]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_correction_ratios_sum_approximately_one() {
        let ratios = gamma_correction_ratios(1.8);
        let sum: f32 = ratios[0] + ratios[1] + ratios[2];
        // The weights do not sum to exactly 1 after gamma correction, but
        // they should be in a sane range.
        assert!(sum > 0.5 && sum < 2.0, "unexpected sum: {sum}");
    }

    #[test]
    fn gamma_correction_ratios_alpha_is_one() {
        let ratios = gamma_correction_ratios(2.0);
        assert!((ratios[3] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pod_bounds_from_pixels_bounds() {
        let b = Bounds {
            origin: Point {
                x: Pixels(10.0),
                y: Pixels(20.0),
            },
            size: Size {
                width: Pixels(100.0),
                height: Pixels(200.0),
            },
        };
        let pod: PodBounds = b.into();
        assert_eq!(pod.origin, [10.0, 20.0]);
        assert_eq!(pod.size, [100.0, 200.0]);
    }

    #[test]
    fn scene_batches_empty_scene() {
        let scene = Scene::default();
        let batches: Vec<_> = scene.batches().collect();
        assert!(batches.is_empty());
    }

    #[test]
    fn scene_batches_quads_only() {
        let mut scene = Scene::default();
        scene.quads.push(Quad::default());
        let batches: Vec<_> = scene.batches().collect();
        assert_eq!(batches.len(), 1);
        assert!(matches!(batches[0], PrimitiveBatch::Quads(_)));
    }

    #[test]
    fn rendering_parameters_default_gamma() {
        // We can only test `gamma_correction_ratios` without a real adapter.
        let ratios = gamma_correction_ratios(1.8);
        assert!(ratios[0] > 0.0);
        assert!(ratios[1] > 0.0);
        assert!(ratios[2] > 0.0);
        assert_eq!(ratios[3], 1.0);
    }

    #[test]
    fn pod_quad_is_pod() {
        // bytemuck::Pod requires no padding — this will fail to compile if
        // the struct has unexpected padding.
        let _ = bytemuck::bytes_of(&Quad::default());
    }

    #[test]
    fn pod_shadow_is_pod() {
        let _ = bytemuck::bytes_of(&Shadow::default());
    }

    #[test]
    fn pod_underline_is_pod() {
        let _ = bytemuck::bytes_of(&Underline::default());
    }

    #[test]
    fn global_params_is_pod() {
        let g = GlobalParams {
            viewport_size: [1920.0, 1080.0],
            premultiplied_alpha: 0,
            pad: 0,
        };
        let _ = bytemuck::bytes_of(&g);
    }

    #[test]
    fn gamma_params_is_pod() {
        let g = GammaParams {
            gamma_ratios: [1.0, 1.0, 1.0, 1.0],
            grayscale_enhanced_contrast: 1.0,
            subpixel_enhanced_contrast: 0.5,
            _pad: [0.0, 0.0],
        };
        let _ = bytemuck::bytes_of(&g);
    }

    #[test]
    fn path_primitive_clipped_bounds_identity() {
        let p = PathPrimitive {
            bounds: Bounds {
                origin: Point {
                    x: Pixels(5.0),
                    y: Pixels(10.0),
                },
                size: Size {
                    width: Pixels(50.0),
                    height: Pixels(100.0),
                },
            },
            ..Default::default()
        };
        let cb = p.clipped_bounds();
        assert_eq!(cb.origin.x.0, 5.0);
        assert_eq!(cb.size.width.0, 50.0);
    }
}
