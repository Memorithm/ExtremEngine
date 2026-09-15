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
    OutOfMemory,
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
    position_bounds: [f64; 4],
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
        if !indices.len().is_multiple_of(3) {
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
        let mut position_bounds = [0.0_f64, 0.0, 0.0, 1.0];
        for vertex in &vertices {
            for (bound, value) in position_bounds.iter_mut().zip(vertex.position) {
                *bound = bound.max(f64::from(value).abs());
            }
        }
        Ok(Arc::new(Self {
            vertices,
            indices,
            position_bounds,
        }))
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

/// Checks matrices and conservative transformed-position bounds before submission.
/// Bounds are cached once per geometry, not recomputed per vertex on every frame.
/// Extreme inputs may be conservatively rejected even when cancellation would yield
/// finite coordinates; this is deliberate rather than backend-dependent clipping.
pub fn validate_frame(camera: &[f32; 16], draws: &[MeshDraw]) -> Result<(), MeshError> {
    if draws.len() > MAX_FRAME_DRAWS {
        return Err(MeshError::Capacity);
    }
    validate_matrix(camera)?;
    for draw in draws {
        draw.validate()?;
        let world_bounds = transform_bounds(&draw.model, draw.mesh.position_bounds)?;
        transform_bounds(camera, world_bounds)?;
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

// Absolute dot-product bounds cover intermediate products and any evaluation order.
// Half the f32 range reserves a large margin for GPU f32 rounding; f64 arithmetic
// prevents the validation calculation itself from overflowing on f32 inputs.
fn transform_bounds(matrix: &[f32; 16], input: [f64; 4]) -> Result<[f64; 4], MeshError> {
    let mut output = [0.0; 4];
    for (row, bound) in output.iter_mut().enumerate() {
        *bound = (0..4)
            .map(|column| f64::from(matrix[column * 4 + row]).abs() * input[column])
            .sum();
        if !bound.is_finite() || *bound > f64::from(f32::MAX) * 0.5 {
            return Err(MeshError::InvalidMatrix);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod bound_tests {
    use super::*;

    fn identity() -> [f32; 16] {
        let mut matrix = [0.0; 16];
        for index in [0, 5, 10, 15] {
            matrix[index] = 1.0;
        }
        matrix
    }

    fn draw(position: [f32; 3]) -> MeshDraw {
        MeshDraw {
            mesh: MeshData::new(
                vec![MeshVertex {
                    position,
                    color: [1.0; 3],
                }],
                vec![0, 0, 0],
            )
            .unwrap(),
            model: identity(),
            color: [1.0; 4],
        }
    }

    #[test]
    fn finite_vertex_and_scale_overflow_is_rejected() {
        let mut item = draw([f32::MAX, 0.0, 0.0]);
        item.model[0] = 2.0;
        assert!(item.validate().is_ok());
        assert_eq!(
            validate_frame(&identity(), &[item]),
            Err(MeshError::InvalidMatrix)
        );
    }

    #[test]
    fn camera_overflow_and_cancellation_are_rejected_conservatively() {
        let item = draw([f32::MAX * 0.25, f32::MAX * 0.25, 0.0]);
        let mut camera = identity();
        camera[0] = 4.0;
        camera[4] = -4.0;
        assert_eq!(
            validate_frame(&camera, &[item]),
            Err(MeshError::InvalidMatrix)
        );
    }

    #[test]
    fn ordinary_negative_scale_translation_and_zero_bounds_are_valid() {
        let mut item = draw([-100.0, 200.0, -0.0]);
        item.model[0] = -2.0;
        item.model[12] = 10_000.0;
        assert!(validate_frame(&identity(), &[item]).is_ok());
        assert!(validate_frame(&identity(), &[draw([0.0; 3])]).is_ok());
    }
}
