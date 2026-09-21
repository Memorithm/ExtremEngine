//! Conservative view-frustum culling for opaque mesh draws.
//!
//! Planes are extracted from a column-major view-projection matrix with WebGPU
//! depth in `[0, 1]`. An AABB is rejected only when it is fully outside at least
//! one plane. Intersecting and fully-inside bounds are kept. Non-finite inputs
//! fail closed as `MeshError::InvalidMatrix`.

use crate::mesh_data::{MeshDraw, MeshError, validate_matrix};

/// Axis-aligned bounding box in a single coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    /// Builds a finite AABB from inclusive corners. Empty or non-finite boxes fail.
    pub fn from_min_max(min: [f32; 3], max: [f32; 3]) -> Result<Self, MeshError> {
        if !min.iter().chain(max.iter()).all(|value| value.is_finite())
            || min[0] > max[0]
            || min[1] > max[1]
            || min[2] > max[2]
        {
            return Err(MeshError::InvalidMatrix);
        }
        Ok(Self { min, max })
    }

    /// Expands an AABB over object-space mesh positions.
    pub fn from_points(points: impl IntoIterator<Item = [f32; 3]>) -> Result<Self, MeshError> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut any = false;
        for point in points {
            if !point.iter().all(|value| value.is_finite()) {
                return Err(MeshError::InvalidMatrix);
            }
            any = true;
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        if !any {
            return Err(MeshError::InvalidMatrix);
        }
        Self::from_min_max(min, max)
    }
}

/// Six inward-facing frustum planes stored as `[nx, ny, nz, d]` for `n·x + d >= 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extracts a WebGPU-depth frustum from a column-major view-projection matrix.
    pub fn from_view_projection(matrix: &[f32; 16]) -> Result<Self, MeshError> {
        validate_matrix(matrix)?;
        // Column-major rows of the clip matrix.
        let r0 = [matrix[0], matrix[4], matrix[8], matrix[12]];
        let r1 = [matrix[1], matrix[5], matrix[9], matrix[13]];
        let r2 = [matrix[2], matrix[6], matrix[10], matrix[14]];
        let r3 = [matrix[3], matrix[7], matrix[11], matrix[15]];

        let mut planes = [
            add(r3, r0), // left
            sub(r3, r0), // right
            add(r3, r1), // bottom
            sub(r3, r1), // top
            r2,          // near (WebGPU z in [0, 1])
            sub(r3, r2), // far
        ];
        for plane in &mut planes {
            normalize_plane(plane)?;
        }
        Ok(Self { planes })
    }

    /// Returns false only when the AABB is completely outside the frustum.
    pub fn intersects_aabb(self, aabb: Aabb) -> bool {
        for plane in self.planes {
            // p-vertex: AABB corner farthest along the plane normal.
            let px = if plane[0] >= 0.0 {
                aabb.max[0]
            } else {
                aabb.min[0]
            };
            let py = if plane[1] >= 0.0 {
                aabb.max[1]
            } else {
                aabb.min[1]
            };
            let pz = if plane[2] >= 0.0 {
                aabb.max[2]
            } else {
                aabb.min[2]
            };
            if plane[0] * px + plane[1] * py + plane[2] * pz + plane[3] < 0.0 {
                return false;
            }
        }
        true
    }
}

/// Transforms a local AABB by a model matrix via its eight corners (conservative).
pub fn transform_aabb(model: &[f32; 16], local: Aabb) -> Result<Aabb, MeshError> {
    validate_matrix(model)?;
    let corners = [
        [local.min[0], local.min[1], local.min[2]],
        [local.max[0], local.min[1], local.min[2]],
        [local.min[0], local.max[1], local.min[2]],
        [local.max[0], local.max[1], local.min[2]],
        [local.min[0], local.min[1], local.max[2]],
        [local.max[0], local.min[1], local.max[2]],
        [local.min[0], local.max[1], local.max[2]],
        [local.max[0], local.max[1], local.max[2]],
    ];
    let mut world = [[0.0_f32; 3]; 8];
    for (index, corner) in corners.into_iter().enumerate() {
        world[index] = transform_point(model, corner)?;
    }
    Aabb::from_points(world)
}

/// Returns indices of draws whose world AABB intersects the camera frustum.
///
/// Draw order is preserved. Fully exterior AABBs are omitted. Invalid matrices or
/// non-finite frustum planes return `MeshError::InvalidMatrix`.
pub fn retain_draws_in_frustum(
    camera: &[f32; 16],
    draws: &[MeshDraw],
) -> Result<Vec<usize>, MeshError> {
    let frustum = Frustum::from_view_projection(camera)?;
    let mut kept = Vec::with_capacity(draws.len());
    for (index, draw) in draws.iter().enumerate() {
        validate_matrix(&draw.model)?;
        let world = transform_aabb(&draw.model, draw.mesh.local_aabb())?;
        if frustum.intersects_aabb(world) {
            kept.push(index);
        }
    }
    Ok(kept)
}

fn add(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

fn sub(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

fn normalize_plane(plane: &mut [f32; 4]) -> Result<(), MeshError> {
    if !plane.iter().all(|value| value.is_finite()) {
        return Err(MeshError::InvalidMatrix);
    }
    let length = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err(MeshError::InvalidMatrix);
    }
    let inverse = length.recip();
    for value in plane.iter_mut() {
        *value *= inverse;
        if !value.is_finite() {
            return Err(MeshError::InvalidMatrix);
        }
    }
    Ok(())
}

fn transform_point(matrix: &[f32; 16], point: [f32; 3]) -> Result<[f32; 3], MeshError> {
    let x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
    let y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
    let z = matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14];
    let w = matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
    if ![x, y, z, w].iter().all(|value| value.is_finite()) {
        return Err(MeshError::InvalidMatrix);
    }
    if w.abs() > f32::EPSILON {
        let inverse = w.recip();
        let out = [x * inverse, y * inverse, z * inverse];
        if !out.iter().all(|value| value.is_finite()) {
            return Err(MeshError::InvalidMatrix);
        }
        Ok(out)
    } else {
        Ok([x, y, z])
    }
}

#[cfg(test)]
mod tests {
    use super::{Aabb, Frustum, retain_draws_in_frustum, transform_aabb};
    use crate::mesh_data::{MeshData, MeshDraw, MeshError, MeshVertex};
    use std::f32::consts::FRAC_PI_2;
    use std::sync::Arc;

    fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
        let f = 1.0 / (fov_y * 0.5).tan();
        let inverse_depth = 1.0 / (near - far);
        [
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            f,
            0.0,
            0.0,
            0.0,
            0.0,
            far * inverse_depth,
            -1.0,
            0.0,
            0.0,
            (far * near) * inverse_depth,
            0.0,
        ]
    }

    fn translation(x: f32, y: f32, z: f32) -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
        ]
    }

    fn unit_cube() -> Arc<MeshData> {
        let vertices = vec![
            MeshVertex {
                position: [-0.5, -0.5, -0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, -0.5, -0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, 0.5, -0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [-0.5, 0.5, -0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [-0.5, -0.5, 0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, -0.5, 0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, 0.5, 0.5],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [-0.5, 0.5, 0.5],
                color: [1.0; 3],
            },
        ];
        MeshData::new(
            vertices,
            vec![
                0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2,
                7, 3, 3, 7, 4, 3, 4, 0,
            ],
        )
        .unwrap()
    }

    #[test]
    fn perspective_frustum_keeps_near_cube_and_rejects_far_side_and_behind() {
        let camera = perspective(FRAC_PI_2, 1.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(&camera).unwrap();
        let local = unit_cube().local_aabb();

        let near = transform_aabb(&translation(0.0, 0.0, -2.0), local).unwrap();
        assert!(frustum.intersects_aabb(near));

        let side = transform_aabb(&translation(50.0, 0.0, -2.0), local).unwrap();
        assert!(!frustum.intersects_aabb(side));

        let behind = transform_aabb(&translation(0.0, 0.0, 2.0), local).unwrap();
        assert!(!frustum.intersects_aabb(behind));

        let beyond_far = transform_aabb(&translation(0.0, 0.0, -200.0), local).unwrap();
        assert!(!frustum.intersects_aabb(beyond_far));
    }

    #[test]
    fn retain_draws_preserves_order_and_counts_only_intersecting() {
        let mesh = unit_cube();
        let camera = perspective(FRAC_PI_2, 1.0, 0.1, 100.0);
        let draws = [
            MeshDraw {
                mesh: Arc::clone(&mesh),
                model: translation(0.0, 0.0, -2.0),
                color: [1.0; 4],
            },
            MeshDraw {
                mesh: Arc::clone(&mesh),
                model: translation(40.0, 0.0, -2.0),
                color: [1.0; 4],
            },
            MeshDraw {
                mesh: Arc::clone(&mesh),
                model: translation(0.0, 0.0, -3.0),
                color: [1.0; 4],
            },
        ];
        assert_eq!(
            retain_draws_in_frustum(&camera, &draws).unwrap(),
            vec![0, 2]
        );
    }

    #[test]
    fn non_finite_view_projection_fails_closed() {
        let mut camera = perspective(FRAC_PI_2, 1.0, 0.1, 100.0);
        camera[0] = f32::NAN;
        assert_eq!(
            Frustum::from_view_projection(&camera),
            Err(MeshError::InvalidMatrix)
        );
    }

    #[test]
    fn translated_aabb_matches_corner_extrema() {
        let local = Aabb::from_min_max([-1.0; 3], [1.0; 3]).unwrap();
        let world = transform_aabb(&translation(2.0, -3.0, 4.0), local).unwrap();
        assert_eq!(world.min, [1.0, -4.0, 3.0]);
        assert_eq!(world.max, [3.0, -2.0, 5.0]);
    }
}
