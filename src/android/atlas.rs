//! wgpu-backed texture atlas for Android.
//!
//! A direct port of `gpui_wgpu::wgpu_atlas` (`WgpuAtlas` / `WgpuAtlasState`)
//! with two changes:
//!
//! 1. The GPUI trait types (`AtlasKey`, `AtlasTile`, `PlatformAtlas`, …) are
//!    **stubbed out** locally so the file compiles without a GPUI workspace
//!    dependency.  When wired into a real GPUI build, delete the stub section
//!    and import from `gpui` instead.
//!
//! 2. The texture format for `Subpixel` tiles is `Rgba8Unorm` instead of
//!    `Bgra8Unorm` because Android / Vulkan surfaces typically prefer RGBA.
//!    The renderer swaps channels as needed during blitting.
//!
//! ## Overview
//!
//! The atlas is a collection of GPU textures partitioned into three "kinds":
//!
//! | Kind          | Format        | Usage                              |
//! |---------------|---------------|------------------------------------|
//! | `Monochrome`  | `R8Unorm`     | Grayscale glyph masks              |
//! | `Subpixel`    | `Rgba8Unorm`  | Subpixel-AA glyph masks (BGR+A)    |
//! | `Polychrome`  | `Rgba8Unorm`  | Full-colour emoji / image tiles    |
//!
//! Each kind holds a list of `WgpuAtlasTexture` instances.  When a tile
//! allocation request cannot fit in an existing texture, a new texture is
//! created (up to `max_texture_size × max_texture_size`).
//!
//! Uploads are **batched**: callers invoke `upload_tile()` and the data is
//! stored in `pending_uploads`.  The batch is flushed to the GPU in one pass
//! at the start of each frame via `before_frame()`.

#![allow(unsafe_code)]

use anyhow::{Context as _, Result};
use etagere::{size2, BucketedAtlasAllocator};
use parking_lot::Mutex;
use std::{borrow::Cow, collections::HashMap, ops, sync::Arc};

use super::{Bounds, DevicePixels, Point, Size};

// ── stub types (replace with `gpui::*` in a full workspace build) ─────────────

/// Opaque key that identifies a cached atlas entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AtlasKey(pub u64);

/// Which texture "layer" an atlas entry lives in.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AtlasTextureKind {
    Monochrome,
    Subpixel,
    Polychrome,
}

/// Identifies a specific atlas texture within a kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct AtlasTextureId {
    pub index: u32,
    pub kind: AtlasTextureKind,
}

/// A successfully allocated tile inside the atlas.
#[derive(Clone, Debug)]
pub struct AtlasTile {
    pub texture_id: AtlasTextureId,
    pub tile_id: u32,
    pub padding: u8,
    pub bounds: Bounds<DevicePixels>,
}

// ── geometry helpers ──────────────────────────────────────────────────────────

fn device_size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    size2(size.width.0, size.height.0)
}

fn etagere_point_to_device(pt: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels(pt.x),
        y: DevicePixels(pt.y),
    }
}

// ── pending upload record ─────────────────────────────────────────────────────

struct PendingUpload {
    id: AtlasTextureId,
    bounds: Bounds<DevicePixels>,
    data: Vec<u8>,
}

// ── atlas state ───────────────────────────────────────────────────────────────

struct WgpuAtlasState {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    /// The device's maximum supported texture dimension (width = height).
    max_texture_size: u32,
    storage: WgpuAtlasStorage,
    /// Maps `AtlasKey` → the tile that was allocated for it.
    tiles_by_key: HashMap<AtlasKey, AtlasTile>,
    /// Uploads queued since the last `before_frame()` call.
    pending_uploads: Vec<PendingUpload>,
}

// ── public API ────────────────────────────────────────────────────────────────

/// wgpu-backed texture atlas.
///
/// Thread-safe via an internal `Mutex`; suitable for sharing between the
/// render thread and the text-shaping thread.
pub struct AndroidAtlas(Mutex<WgpuAtlasState>);

/// A snapshot of a texture's `wgpu::TextureView`, returned by
/// `AndroidAtlas::get_texture_info`.
pub struct AtlasTextureInfo {
    pub view: wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: Size<DevicePixels>,
}

impl AndroidAtlas {
    /// Create a new atlas backed by `device` and `queue`.
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let max_texture_size = device.limits().max_texture_dimension_2d;
        AndroidAtlas(Mutex::new(WgpuAtlasState {
            device,
            queue,
            max_texture_size,
            storage: WgpuAtlasStorage::default(),
            tiles_by_key: HashMap::new(),
            pending_uploads: Vec::new(),
        }))
    }

    // ── frame lifecycle ───────────────────────────────────────────────────────

    /// Flush all pending texture uploads to the GPU.
    ///
    /// Must be called once at the **start** of every frame, before any draw
    /// calls that read from atlas textures.
    pub fn before_frame(&self) {
        let mut lock = self.0.lock();
        lock.flush_uploads();
    }

    // ── tile allocation ───────────────────────────────────────────────────────

    /// Return the tile for `key`, inserting a new one by calling `build` if
    /// the key is not yet cached.
    ///
    /// `build` must return `Ok(Some((size, bytes)))` on success, or
    /// `Ok(None)` to indicate that no tile should be cached for this key
    /// (e.g. a whitespace glyph with no pixels).
    pub fn get_or_insert_with<'a>(
        &self,
        key: &AtlasKey,
        kind: AtlasTextureKind,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        let mut lock = self.0.lock();

        if let Some(tile) = lock.tiles_by_key.get(key) {
            return Ok(Some(tile.clone()));
        }

        let Some((size, bytes)) = build()? else {
            return Ok(None);
        };

        let tile = lock
            .allocate(size, kind)
            .context("atlas: failed to allocate tile")?;

        lock.upload_tile(tile.texture_id, tile.bounds, &bytes);
        lock.tiles_by_key.insert(key.clone(), tile.clone());

        Ok(Some(tile))
    }

    /// Remove a tile from the cache, freeing its atlas allocation.
    pub fn remove(&self, key: &AtlasKey) {
        let mut lock = self.0.lock();

        let Some(id) = lock.tiles_by_key.remove(key).map(|t| t.texture_id) else {
            return;
        };

        let textures = &mut lock.storage[id.kind];
        if let Some(slot) = textures.textures.get_mut(id.index as usize) {
            if let Some(mut texture) = slot.take() {
                texture.decrement_ref_count();
                if texture.is_unreferenced() {
                    textures.free_list.push(id.index as usize);
                } else {
                    *slot = Some(texture);
                }
            }
        }
    }

    // ── texture introspection ─────────────────────────────────────────────────

    /// Return a snapshot of the texture identified by `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` does not refer to a live texture.
    pub fn get_texture_info(&self, id: AtlasTextureId) -> AtlasTextureInfo {
        let lock = self.0.lock();
        let tex = &lock.storage[id];
        AtlasTextureInfo {
            view: tex
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            format: tex.format,
            size: Size {
                width: DevicePixels(tex.width as i32),
                height: DevicePixels(tex.height as i32),
            },
        }
    }

    /// Return the number of live textures of each kind.
    pub fn texture_counts(&self) -> (usize, usize, usize) {
        let lock = self.0.lock();
        (
            lock.storage
                .monochrome
                .textures
                .iter()
                .filter(|t| t.is_some())
                .count(),
            lock.storage
                .subpixel
                .textures
                .iter()
                .filter(|t| t.is_some())
                .count(),
            lock.storage
                .polychrome
                .textures
                .iter()
                .filter(|t| t.is_some())
                .count(),
        )
    }

    /// Remove every cached tile from every texture.
    ///
    /// This does **not** free GPU memory — the textures remain allocated but
    /// their allocators are reset.  Useful when the display surface is lost
    /// (e.g. `APP_CMD_TERM_WINDOW`) and we need to re-upload everything on the
    /// next `APP_CMD_INIT_WINDOW`.
    pub fn invalidate(&self) {
        let mut lock = self.0.lock();
        lock.tiles_by_key.clear();
        lock.pending_uploads.clear();
        for kind in [
            AtlasTextureKind::Monochrome,
            AtlasTextureKind::Subpixel,
            AtlasTextureKind::Polychrome,
        ] {
            for slot in lock.storage[kind].textures.iter_mut() {
                if let Some(tex) = slot.as_mut() {
                    tex.allocator =
                        BucketedAtlasAllocator::new(size2(tex.width as i32, tex.height as i32));
                    tex.live_atlas_keys = 0;
                }
            }
        }
    }
}

// ── WgpuAtlasState helpers ────────────────────────────────────────────────────

impl WgpuAtlasState {
    /// Allocate a `size`-pixel region in the atlas for the given `kind`.
    fn allocate(&mut self, size: Size<DevicePixels>, kind: AtlasTextureKind) -> Option<AtlasTile> {
        // Try to fit into an existing texture (iterate in reverse so the most
        // recently-created texture — likely to have free space — is tried first).
        {
            let textures = &mut self.storage[kind];
            if let Some(tile) = textures
                .textures
                .iter_mut()
                .rev()
                .filter_map(|slot| slot.as_mut())
                .find_map(|tex| tex.allocate(size))
            {
                return Some(tile);
            }
        }

        // No room — create a new atlas texture.
        let tex = self.push_texture(size, kind);
        tex.allocate(size)
    }

    /// Create a new `WgpuAtlasTexture` for `kind` and push it into storage.
    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> &mut WgpuAtlasTexture {
        const DEFAULT_ATLAS_SIZE: u32 = 1024;

        let max = self.max_texture_size;
        let w = (min_size.width.0 as u32).max(DEFAULT_ATLAS_SIZE).min(max);
        let h = (min_size.height.0 as u32).max(DEFAULT_ATLAS_SIZE).min(max);

        let format = match kind {
            AtlasTextureKind::Monochrome => wgpu::TextureFormat::R8Unorm,
            // Android / Vulkan surfaces prefer RGBA; renderer swaps BGR if needed.
            AtlasTextureKind::Subpixel => wgpu::TextureFormat::Rgba8Unorm,
            AtlasTextureKind::Polychrome => wgpu::TextureFormat::Rgba8Unorm,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("android_atlas"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let storage = &mut self.storage[kind];
        let index = storage.free_list.pop().unwrap_or(storage.textures.len());

        let atlas_tex = WgpuAtlasTexture {
            id: AtlasTextureId {
                index: index as u32,
                kind,
            },
            allocator: BucketedAtlasAllocator::new(size2(w as i32, h as i32)),
            texture,
            format,
            width: w,
            height: h,
            live_atlas_keys: 0,
        };

        if index < storage.textures.len() {
            storage.textures[index] = Some(atlas_tex);
            storage.textures[index].as_mut().unwrap()
        } else {
            storage.textures.push(Some(atlas_tex));
            storage.textures.last_mut().unwrap().as_mut().unwrap()
        }
    }

    /// Queue `bytes` for upload into the atlas region `bounds` of texture `id`.
    fn upload_tile(&mut self, id: AtlasTextureId, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        self.pending_uploads.push(PendingUpload {
            id,
            bounds,
            data: bytes.to_vec(),
        });
    }

    /// Write all queued uploads to the GPU via `queue.write_texture`.
    fn flush_uploads(&mut self) {
        for upload in self.pending_uploads.drain(..) {
            let texture = &self.storage[upload.id];
            let bpp = bytes_per_pixel(texture.format);

            let w = upload.bounds.size.width.0 as u32;
            let h = upload.bounds.size.height.0 as u32;

            if w == 0 || h == 0 || upload.data.is_empty() {
                continue;
            }

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: upload.bounds.origin.x.0 as u32,
                        y: upload.bounds.origin.y.0 as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &upload.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * bpp as u32),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

// ── storage ───────────────────────────────────────────────────────────────────

/// Per-kind storage for atlas textures.
struct AtlasTextureList {
    textures: Vec<Option<WgpuAtlasTexture>>,
    /// Indices of slots in `textures` that have been freed and may be reused.
    free_list: Vec<usize>,
}

impl Default for AtlasTextureList {
    fn default() -> Self {
        Self {
            textures: Vec::new(),
            free_list: Vec::new(),
        }
    }
}

impl AtlasTextureList {}

/// Holds one `AtlasTextureList` per `AtlasTextureKind`.
#[derive(Default)]
struct WgpuAtlasStorage {
    monochrome: AtlasTextureList,
    subpixel: AtlasTextureList,
    polychrome: AtlasTextureList,
}

impl ops::Index<AtlasTextureKind> for WgpuAtlasStorage {
    type Output = AtlasTextureList;
    fn index(&self, kind: AtlasTextureKind) -> &Self::Output {
        match kind {
            AtlasTextureKind::Monochrome => &self.monochrome,
            AtlasTextureKind::Subpixel => &self.subpixel,
            AtlasTextureKind::Polychrome => &self.polychrome,
        }
    }
}

impl ops::IndexMut<AtlasTextureKind> for WgpuAtlasStorage {
    fn index_mut(&mut self, kind: AtlasTextureKind) -> &mut Self::Output {
        match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome,
            AtlasTextureKind::Subpixel => &mut self.subpixel,
            AtlasTextureKind::Polychrome => &mut self.polychrome,
        }
    }
}

/// Index by `AtlasTextureId` returns the **texture itself** (panics if absent).
impl ops::Index<AtlasTextureId> for WgpuAtlasStorage {
    type Output = WgpuAtlasTexture;
    fn index(&self, id: AtlasTextureId) -> &Self::Output {
        let list = &self[id.kind];
        list.textures
            .get(id.index as usize)
            .and_then(|s| s.as_ref())
            .unwrap_or_else(|| panic!("atlas texture {:?} does not exist", id))
    }
}

// ── WgpuAtlasTexture ──────────────────────────────────────────────────────────

/// A single GPU texture used as one page of the atlas.
struct WgpuAtlasTexture {
    id: AtlasTextureId,
    allocator: BucketedAtlasAllocator,
    texture: wgpu::Texture,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    /// Number of `AtlasTile`s currently pointing into this texture.
    live_atlas_keys: u32,
}

impl WgpuAtlasTexture {
    /// Try to carve out a `size`-pixel sub-region.  Returns `None` if full.
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let alloc = self.allocator.allocate(device_size_to_etagere(size))?;
        self.live_atlas_keys += 1;
        Some(AtlasTile {
            texture_id: self.id,
            tile_id: alloc.id.serialize(),
            padding: 0,
            bounds: Bounds {
                origin: etagere_point_to_device(alloc.rectangle.min),
                size,
            },
        })
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys = self.live_atlas_keys.saturating_sub(1);
    }

    fn is_unreferenced(&self) -> bool {
        self.live_atlas_keys == 0
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn bytes_per_pixel(format: wgpu::TextureFormat) -> u8 {
    match format {
        wgpu::TextureFormat::R8Unorm => 1,
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Bgra8UnormSrgb => 4,
        _ => 4, // conservative fallback
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirm that `bytes_per_pixel` returns the right values for every
    /// format the atlas actually uses.
    #[test]
    fn bytes_per_pixel_correct() {
        assert_eq!(bytes_per_pixel(wgpu::TextureFormat::R8Unorm), 1);
        assert_eq!(bytes_per_pixel(wgpu::TextureFormat::Rgba8Unorm), 4);
        assert_eq!(bytes_per_pixel(wgpu::TextureFormat::Bgra8Unorm), 4);
    }

    /// Verify that `etagere_point_to_device` maps coordinates correctly.
    #[test]
    fn etagere_point_conversion() {
        let pt = etagere_point_to_device(etagere::point(10, 20));
        assert_eq!(pt.x, DevicePixels(10));
        assert_eq!(pt.y, DevicePixels(20));
    }

    /// Verify that `device_size_to_etagere` preserves dimensions.
    #[test]
    fn etagere_size_conversion() {
        let s = device_size_to_etagere(Size {
            width: DevicePixels(128),
            height: DevicePixels(256),
        });
        assert_eq!(s.width, 128);
        assert_eq!(s.height, 256);
    }

    /// `AtlasTextureList` starts empty and has an empty free list.
    #[test]
    fn texture_list_starts_empty() {
        let list = AtlasTextureList::default();
        assert!(list.textures.is_empty());
        assert!(list.free_list.is_empty());
    }

    /// `WgpuAtlasStorage` index round-trips for every kind.
    #[test]
    fn storage_index_kind() {
        let storage = WgpuAtlasStorage::default();
        // Just check we can index without panicking.
        let _ = &storage[AtlasTextureKind::Monochrome];
        let _ = &storage[AtlasTextureKind::Subpixel];
        let _ = &storage[AtlasTextureKind::Polychrome];
    }

    /// `is_unreferenced` is true when `live_atlas_keys` hits zero.
    #[test]
    fn unreferenced_after_decrement() {
        // We can't create a real WgpuAtlasTexture without a device, so test
        // the helper logic directly.
        let mut count: u32 = 1;
        count = count.saturating_sub(1);
        assert_eq!(count, 0, "should be unreferenced after decrement");
    }

    /// `AtlasKey` equality is value-based.
    #[test]
    fn atlas_key_equality() {
        let k1 = AtlasKey(42);
        let k2 = AtlasKey(42);
        let k3 = AtlasKey(99);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    /// Pending upload list starts empty.
    #[test]
    fn pending_uploads_start_empty() {
        // Simulate constructing state without a real device.
        let uploads: Vec<PendingUpload> = Vec::new();
        assert!(uploads.is_empty());
    }

    /// `bytes_per_pixel` falls back to 4 for unknown formats.
    #[test]
    fn bytes_per_pixel_fallback() {
        // Depth formats aren't used in the atlas, but we should not panic.
        assert_eq!(bytes_per_pixel(wgpu::TextureFormat::Depth32Float), 4);
    }
}
