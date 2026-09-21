//! Validated indexed geometry, normals, lighting and bounded frame inputs.
use std::fmt;
use std::sync::Arc;

pub const MAX_MESH_VERTICES: usize = 1_000_000;
pub const MAX_MESH_INDICES: usize = 3_000_000;
pub const MAX_FRAME_DRAWS: usize = 4096;
pub const MAX_RESIDENT_MESHES: usize = 256;
pub const MAX_GEOMETRY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TARGET_PIXELS: u64 = 8_388_608;
pub const MAX_LIGHT_INTENSITY: f32 = 16.0;

/// A position and linear RGB vertex color. Geometry is triangle-list only.
/// Normals are stored separately in `MeshData` so the original vertex API remains compatible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// One validated world-space directional light used by the native mesh path.
/// `direction_to_light` points from the shaded surface toward the light source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshLight {
    pub direction_to_light: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub ambient: f32,
}

impl Default for MeshLight {
    /// Compatibility lighting: pure ambient white reproduces the previous unlit colors.
    fn default() -> Self {
        Self {
            direction_to_light: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            intensity: 0.0,
            ambient: 1.0,
        }
    }
}

impl MeshLight {
    pub fn validate(self) -> Result<Self, MeshError> {
        let direction_length_squared = length_squared(self.direction_to_light);
        if !self
            .direction_to_light
            .iter()
            .chain(self.color.iter())
            .all(|value| value.is_finite())
            || !self.intensity.is_finite()
            || !self.ambient.is_finite()
            || self.intensity < 0.0
            || self.intensity > MAX_LIGHT_INTENSITY
            || !(0.0..=1.0).contains(&self.ambient)
            || self.color.iter().any(|value| !(0.0..=1.0).contains(value))
            || !direction_length_squared.is_finite()
            || direction_length_squared <= f32::EPSILON
        {
            return Err(MeshError::InvalidLight);
        }
        Ok(self)
    }
}

/// CPU validation, capacity or GPU/readback failure; never a successful draw count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    EmptyGeometry,
    InvalidIndexCount,
    IndexOutOfBounds,
    InvalidVertex,
    InvalidNormal,
    InvalidNormalTransform,
    InvalidMatrix,
    InvalidColor,
    InvalidLight,
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
/// `new` derives area-weighted smooth normals from triangle winding. Vertices touched
/// only by degenerate triangles receive a +Z fallback normal so legacy degenerate test
/// geometry remains accepted. `new_with_normals` accepts explicit normals for authored
/// hard edges. Explicit normals are normalized once during construction.
#[derive(Debug)]
pub struct MeshData {
    vertices: Vec<MeshVertex>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    position_bounds: [f64; 4],
    local_aabb: crate::frustum::Aabb,
}

impl MeshData {
    /// Constructs finite triangle-list geometry and derives vertex normals.
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
    /// assert!(mesh.normals()[0][2] > 0.99);
    /// # Ok::<(), extrem_gpu::MeshError>(())
    /// ```
    pub fn new(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Result<Arc<Self>, MeshError> {
        validate_geometry(&vertices, &indices)?;
        let normals = generate_normals(&vertices, &indices);
        Self::finish(vertices, normals, indices)
    }

    /// Constructs geometry with explicit object-space vertex normals.
    pub fn new_with_normals(
        vertices: Vec<MeshVertex>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
    ) -> Result<Arc<Self>, MeshError> {
        validate_geometry(&vertices, &indices)?;
        if normals.len() != vertices.len() {
            return Err(MeshError::InvalidNormal);
        }
        let normals = normals
            .into_iter()
            .map(normalize_normal)
            .collect::<Result<Vec<_>, _>>()?;
        Self::finish(vertices, normals, indices)
    }

    fn finish(
        vertices: Vec<MeshVertex>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
    ) -> Result<Arc<Self>, MeshError> {
        let mut position_bounds = [0.0_f64, 0.0, 0.0, 1.0];
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in &vertices {
            for (bound, value) in position_bounds.iter_mut().zip(vertex.position) {
                *bound = bound.max(f64::from(value).abs());
            }
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        let local_aabb = crate::frustum::Aabb::from_min_max(min, max)?;
        Ok(Arc::new(Self {
            vertices,
            normals,
            indices,
            position_bounds,
            local_aabb,
        }))
    }

    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Object-space axis-aligned bounds used by conservative frustum culling.
    pub fn local_aabb(&self) -> crate::frustum::Aabb {
        self.local_aabb
    }

    /// Exact uploaded vertex/normal/index payload bytes; excludes allocator/driver overhead.
    pub fn payload_bytes(&self) -> usize {
        self.vertices.len() * 36 + self.indices.len() * 4
    }
}

fn validate_geometry(vertices: &[MeshVertex], indices: &[u32]) -> Result<(), MeshError> {
    if vertices.is_empty() || indices.is_empty() {
        return Err(MeshError::EmptyGeometry);
    }
    if vertices.len() > MAX_MESH_VERTICES || indices.len() > MAX_MESH_INDICES {
        return Err(MeshError::Capacity);
    }
    if !indices.len().is_multiple_of(3) {
        return Err(MeshError::InvalidIndexCount);
    }
    if vertices.iter().any(|vertex| {
        !vertex.position.iter().all(|value| value.is_finite())
            || !vertex
                .color
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    }) {
        return Err(MeshError::InvalidVertex);
    }
    if indices
        .iter()
        .any(|&index| index as usize >= vertices.len())
    {
        return Err(MeshError::IndexOutOfBounds);
    }
    Ok(())
}

fn generate_normals(vertices: &[MeshVertex], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut accumulated = vec![[0.0_f64; 3]; vertices.len()];
    for triangle in indices.chunks_exact(3) {
        let a = vertices[triangle[0] as usize].position.map(f64::from);
        let b = vertices[triangle[1] as usize].position.map(f64::from);
        let c = vertices[triangle[2] as usize].position.map(f64::from);
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let face = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        if face.iter().all(|value| value.is_finite()) {
            for &index in triangle {
                let normal = &mut accumulated[index as usize];
                for axis in 0..3 {
                    normal[axis] += face[axis];
                }
            }
        }
    }
    accumulated
        .into_iter()
        .map(|normal| {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if length.is_finite() && length > 0.0 {
                [
                    (normal[0] / length) as f32,
                    (normal[1] / length) as f32,
                    (normal[2] / length) as f32,
                ]
            } else {
                [0.0, 0.0, 1.0]
            }
        })
        .collect()
}

fn normalize_normal(normal: [f32; 3]) -> Result<[f32; 3], MeshError> {
    if !normal.iter().all(|value| value.is_finite()) {
        return Err(MeshError::InvalidNormal);
    }
    let length_squared = length_squared(normal);
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return Err(MeshError::InvalidNormal);
    }
    let inverse = length_squared.sqrt().recip();
    Ok(normal.map(|value| value * inverse))
}

/// One opaque vertex-colored draw. Matrices are column-major, as in extrem_math.
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
        validate_normal_transform(&self.model)?;
        if !self
            .color
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            || self.color[3] != 1.0
        {
            return Err(MeshError::InvalidColor);
        }
        Ok(())
    }
}

pub fn validate_matrix(matrix: &[f32; 16]) -> Result<(), MeshError> {
    if !matrix.iter().all(|value| value.is_finite()) {
        return Err(MeshError::InvalidMatrix);
    }
    Ok(())
}

/// Rejects singular or f32-overflowing model bases before the shader constructs a normal matrix.
pub fn validate_normal_transform(model: &[f32; 16]) -> Result<(), MeshError> {
    let c0 = [model[0], model[1], model[2]];
    let c1 = [model[4], model[5], model[6]];
    let c2 = [model[8], model[9], model[10]];
    let cofactors = [cross(c1, c2), cross(c2, c0), cross(c0, c1)];
    if cofactors.iter().flatten().any(|value| !value.is_finite()) {
        return Err(MeshError::InvalidNormalTransform);
    }
    let determinant = dot(c0, cofactors[0]);
    if !determinant.is_finite() || determinant.abs() < f32::MIN_POSITIVE {
        return Err(MeshError::InvalidNormalTransform);
    }
    Ok(())
}

/// Compatibility validation using ambient-only lighting.
pub fn validate_frame(camera: &[f32; 16], draws: &[MeshDraw]) -> Result<(), MeshError> {
    validate_lit_frame(camera, MeshLight::default(), draws)
}

/// Checks lighting, matrices and conservative transformed-position bounds before submission.
pub fn validate_lit_frame(
    camera: &[f32; 16],
    light: MeshLight,
    draws: &[MeshDraw],
) -> Result<(), MeshError> {
    if draws.len() > MAX_FRAME_DRAWS {
        return Err(MeshError::Capacity);
    }
    validate_matrix(camera)?;
    light.validate()?;
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

/// CPU reference for the shader's normal transform, including mirrored models.
pub fn transform_normal_reference(
    model: &[f32; 16],
    normal: [f32; 3],
) -> Result<[f32; 3], MeshError> {
    validate_normal_transform(model)?;
    let normal = normalize_normal(normal)?;
    let c0 = [model[0], model[1], model[2]];
    let c1 = [model[4], model[5], model[6]];
    let c2 = [model[8], model[9], model[10]];
    let cof0 = cross(c1, c2);
    let cof1 = cross(c2, c0);
    let cof2 = cross(c0, c1);
    let orientation = if dot(c0, cof0).is_sign_negative() {
        -1.0
    } else {
        1.0
    };
    normalize_normal([
        orientation * (cof0[0] * normal[0] + cof1[0] * normal[1] + cof2[0] * normal[2]),
        orientation * (cof0[1] * normal[0] + cof1[1] * normal[1] + cof2[1] * normal[2]),
        orientation * (cof0[2] * normal[0] + cof1[2] * normal[1] + cof2[2] * normal[2]),
    ])
}

/// CPU Lambert reference for pixel qualification. Values are linear and clamped to one.
pub fn shade_lambert(
    base_color: [f32; 3],
    world_normal: [f32; 3],
    light: MeshLight,
) -> Result<[f32; 3], MeshError> {
    if !base_color
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return Err(MeshError::InvalidColor);
    }
    let light = light.validate()?;
    let normal = normalize_normal(world_normal)?;
    let direction = normalize_normal(light.direction_to_light)?;
    let diffuse = dot(normal, direction).max(0.0) * light.intensity;
    Ok(std::array::from_fn(|axis| {
        (base_color[axis] * (light.ambient + light.color[axis] * diffuse)).clamp(0.0, 1.0)
    }))
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

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length_squared(value: [f32; 3]) -> f32 {
    dot(value, value)
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
        assert_eq!(
            validate_frame(&identity(), &[item]),
            Err(MeshError::InvalidMatrix)
        );
    }

    #[test]
    fn overflowing_finite_normal_and_light_directions_are_rejected() {
        let vertices = vec![MeshVertex {
            position: [0.0; 3],
            color: [1.0; 3],
        }];
        assert!(matches!(
            MeshData::new_with_normals(vertices, vec![[f32::MAX, 0.0, 0.0]], vec![0, 0, 0]),
            Err(MeshError::InvalidNormal)
        ));

        let light = MeshLight {
            direction_to_light: [f32::MAX, 0.0, 0.0],
            color: [1.0; 3],
            intensity: 1.0,
            ambient: 0.0,
        };
        assert_eq!(light.validate(), Err(MeshError::InvalidLight));
        assert_eq!(
            shade_lambert([1.0; 3], [0.0, 0.0, 1.0], light),
            Err(MeshError::InvalidLight)
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
