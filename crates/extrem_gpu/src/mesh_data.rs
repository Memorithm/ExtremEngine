//! Validated indexed geometry and bounded frame inputs for the native mesh path.
use std::fmt;
use std::sync::Arc;

pub const MAX_MESH_VERTICES: usize = 1_000_000;
pub const MAX_MESH_INDICES: usize = 3_000_000;
pub const MAX_FRAME_DRAWS: usize = 4096;
pub const MAX_RESIDENT_MESHES: usize = 256;
pub const MAX_GEOMETRY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TARGET_PIXELS: u64 = 8_388_608;

/// A position and linear RGB vertex color. Geometry is triangle-list only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// CPU validation, capacity or GPU/readback failure; never a successful draw count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    EmptyGeometry,
    InvalidIndexCount,
    IndexOutOfBounds,
    InvalidVertex,
    InvalidMatrix,
    InvalidColor,
    MissingCamera,
    MissingExtraction,
    Capacity,
    InvalidExtent,
    ReadbackUnavailable,
    Gpu(String),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gpu(message) => write!(f, "mesh GPU operation failed: {message}"),
            other => write!(f, "mesh input rejected: {other:?}"),
        }
    }
}

impl std::error::Error for MeshError {}

/// Immutable validated geometry, shareable between entities and GPU cache entries.
///
/// Construction validates before taking an Arc snapshot. Degenerate triangles are
/// permitted; this is index/numeric validation, not manifold or topology repair.
/// The limits bound accepted geometry, not the caller's already-allocated vectors.
#[derive(Debug)]
pub struct MeshData {
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl MeshData {
    /// Constructs finite triangle-list geometry with in-bounds u32 indices.
    ///
    /// # Errors
    /// Rejects empty, oversized, non-finite, out-of-range-color or invalid indices.
    ///
    /// # Examples
    /// ```
    /// use extrem_gpu::{MeshData, MeshVertex};
    /// let vertices = vec![
    ///     MeshVertex { position: [-0.5, -0.5, 0.0], color: [1.0; 3] },
    ///     MeshVertex { position: [0.5, -0.5, 0.0], color: [1.0; 3] },
    ///     MeshVertex { position: [0.0, 0.5, 0.0], color: [1.0; 3] },
    /// ];
    /// let mesh = MeshData::new(vertices, vec![0, 1, 2])?;
    /// assert_eq!(mesh.indices(), [0, 1, 2]);
    /// # Ok::<(), extrem_gpu::MeshError>(())
    /// ```
    pub fn new(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Result<Arc<Self>, MeshError> {
        if vertices.is_empty() || indices.is_empty() {
            return Err(MeshError::EmptyGeometry);
        }
        if vertices.len() > MAX_MESH_VERTICES || indices.len() > MAX_MESH_INDICES {
            return Err(MeshError::Capacity);
        }
        if indices.len() % 3 != 0 {
            return Err(MeshError::InvalidIndexCount);
        }
        if vertices.iter().any(|v| {
            !v.position.iter().all(|x| x.is_finite())
                || !v
                    .color
                    .iter()
                    .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
        }) {
            return Err(MeshError::InvalidVertex);
        }
        if indices
            .iter()
            .any(|&index| index as usize >= vertices.len())
        {
            return Err(MeshError::IndexOutOfBounds);
        }
        Ok(Arc::new(Self { vertices, indices }))
    }

    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Exact vertex/index payload bytes; not allocator/GPU-driver overhead.
    pub fn payload_bytes(&self) -> usize {
        self.vertices.len() * 24 + self.indices.len() * 4
    }
}

/// One opaque, vertex-colored draw. Matrices are column-major, as in extrem_math.
#[derive(Clone, Debug)]
pub struct MeshDraw {
    pub mesh: Arc<MeshData>,
    pub model: [f32; 16],
    /// Linear RGBA multiplier. Alpha must be one; transparency is not implemented.
    pub color: [f32; 4],
}

impl MeshDraw {
    pub fn validate(&self) -> Result<(), MeshError> {
        validate_matrix(&self.model)?;
        if !self
            .color
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            || self.color[3] != 1.0
        {
            return Err(MeshError::InvalidColor);
        }
        Ok(())
    }
}

pub fn validate_matrix(matrix: &[f32; 16]) -> Result<(), MeshError> {
    if !matrix.iter().all(|x| x.is_finite()) {
        return Err(MeshError::InvalidMatrix);
    }
    Ok(())
}

/// Checks the full MVP numeric boundary without changing matrix multiplication order.
/// Finite input matrices can overflow on composition, so test their product too.
pub fn validate_frame(camera: &[f32; 16], draws: &[MeshDraw]) -> Result<(), MeshError> {
    if draws.len() > MAX_FRAME_DRAWS {
        return Err(MeshError::Capacity);
    }
    validate_matrix(camera)?;
    for draw in draws {
        draw.validate()?;
        for column in 0..4 {
            for row in 0..4 {
                let value: f32 = (0..4)
                    .map(|k| camera[k * 4 + row] * draw.model[column * 4 + k])
                    .sum();
                if !value.is_finite() {
                    return Err(MeshError::InvalidMatrix);
                }
            }
        }
    }
    Ok(())
}

pub fn validate_extent(width: u32, height: u32, device_limit: u32) -> Result<(), MeshError> {
    if width == 0
        || height == 0
        || width > device_limit
        || height > device_limit
        || u64::from(width) * u64::from(height) > MAX_TARGET_PIXELS
    {
        return Err(MeshError::InvalidExtent);
    }
    Ok(())
}
