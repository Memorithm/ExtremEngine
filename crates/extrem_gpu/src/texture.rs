//! Bounded CPU-side texture/material contracts for the native mesh pipeline.
use std::fmt;
use std::sync::Arc;

/// Per-texture dimension cap. Importers must downscale or reject larger inputs explicitly.
pub const MAX_TEXTURE_DIMENSION: u32 = 4096;
/// Per-texture pixel cap, allowing one 4096x4096 RGBA8 image.
pub const MAX_TEXTURE_PIXELS: u64 = 16_777_216;
/// Exact maximum accepted RGBA8 payload bytes for one texture.
pub const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;

/// How RGB bytes are interpreted before lighting. Alpha remains linear in both modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureColorSpace {
    Linear,
    Srgb,
}

/// Fail-closed validation errors for texture/material data before GPU upload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    InvalidExtent,
    InvalidDataLength,
    InvalidUv,
    InvalidMaterial,
    Capacity,
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "texture/material input rejected: {self:?}")
    }
}

impl std::error::Error for TextureError {}

/// Immutable validated RGBA8 texture payload.
///
/// Texel `(0, 0)` is the first four payload bytes. UV sampling uses a top-left origin:
/// `(0, 0)` addresses the first texel and `(1, 1)` addresses the last texel.
#[derive(Debug)]
pub struct TextureData {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
    color_space: TextureColorSpace,
}

impl TextureData {
    /// Validates extent and exact RGBA8 payload length before taking ownership.
    pub fn new_rgba8(
        width: u32,
        height: u32,
        rgba8: Vec<u8>,
        color_space: TextureColorSpace,
    ) -> Result<Arc<Self>, TextureError> {
        if width == 0
            || height == 0
            || width > MAX_TEXTURE_DIMENSION
            || height > MAX_TEXTURE_DIMENSION
        {
            return Err(TextureError::InvalidExtent);
        }
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(TextureError::Capacity)?;
        if pixels > MAX_TEXTURE_PIXELS {
            return Err(TextureError::Capacity);
        }
        let bytes = pixels.checked_mul(4).ok_or(TextureError::Capacity)?;
        let bytes = usize::try_from(bytes).map_err(|_| TextureError::Capacity)?;
        if bytes > MAX_TEXTURE_BYTES {
            return Err(TextureError::Capacity);
        }
        if rgba8.len() != bytes {
            return Err(TextureError::InvalidDataLength);
        }
        Ok(Arc::new(Self {
            width,
            height,
            rgba8,
            color_space,
        }))
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    pub fn color_space(&self) -> TextureColorSpace {
        self.color_space
    }

    /// Exact texture payload bytes, excluding allocator/GPU-driver overhead.
    pub fn payload_bytes(&self) -> usize {
        self.rgba8.len()
    }

    /// CPU nearest-neighbour clamp sampler used as a future GPU pixel-reference contract.
    pub fn sample_nearest_clamp(&self, uv: [f32; 2]) -> Result<[u8; 4], TextureError> {
        if !uv.iter().all(|value| value.is_finite()) {
            return Err(TextureError::InvalidUv);
        }
        let x = nearest_coordinate(uv[0], self.width);
        let y = nearest_coordinate(uv[1], self.height);
        let texel = u64::from(y)
            .checked_mul(u64::from(self.width))
            .and_then(|row| row.checked_add(u64::from(x)))
            .and_then(|index| index.checked_mul(4))
            .ok_or(TextureError::Capacity)?;
        let texel = usize::try_from(texel).map_err(|_| TextureError::Capacity)?;
        let bytes = self
            .rgba8
            .get(texel..texel + 4)
            .ok_or(TextureError::InvalidDataLength)?;
        Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    /// Samples and decodes RGB to linear `[0, 1]`; alpha remains linear.
    pub fn sample_linear_nearest_clamp(&self, uv: [f32; 2]) -> Result<[f32; 4], TextureError> {
        let texel = self.sample_nearest_clamp(uv)?;
        let mut result = [0.0; 4];
        for axis in 0..3 {
            let encoded = f32::from(texel[axis]) / 255.0;
            result[axis] = match self.color_space {
                TextureColorSpace::Linear => encoded,
                TextureColorSpace::Srgb => srgb_to_linear(encoded),
            };
        }
        result[3] = f32::from(texel[3]) / 255.0;
        Ok(result)
    }
}

fn nearest_coordinate(value: f32, extent: u32) -> u32 {
    let last = extent - 1;
    let coordinate = (value.clamp(0.0, 1.0) * extent as f32).floor() as u32;
    coordinate.min(last)
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Opaque material data for the existing mesh path.
///
/// `base_color` is a linear RGBA multiplier. Alpha must stay one until the renderer gains an
/// explicit transparency contract. `albedo` is optional so legacy vertex-colored meshes remain
/// representable without inventing a texture.
#[derive(Clone, Debug)]
pub struct MeshMaterial {
    pub base_color: [f32; 4],
    pub albedo: Option<Arc<TextureData>>,
}

impl Default for MeshMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            albedo: None,
        }
    }
}

impl MeshMaterial {
    pub fn validate(&self) -> Result<(), TextureError> {
        if !self
            .base_color
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            || self.base_color[3] != 1.0
        {
            return Err(TextureError::InvalidMaterial);
        }
        Ok(())
    }

    /// CPU reference for future shader material evaluation.
    pub fn sample_base_color(&self, uv: [f32; 2]) -> Result<[f32; 4], TextureError> {
        self.validate()?;
        let texel = match &self.albedo {
            Some(texture) => texture.sample_linear_nearest_clamp(uv)?,
            None => [1.0; 4],
        };
        let mut result = [0.0; 4];
        for axis in 0..3 {
            result[axis] = (texel[axis] * self.base_color[axis]).clamp(0.0, 1.0);
        }
        result[3] = 1.0;
        Ok(result)
    }
}
