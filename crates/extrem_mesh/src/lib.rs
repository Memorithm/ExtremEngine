//! Mesh types, vertex layouts, and mesh handles for ExtremEngine.

#![forbid(unsafe_code)]

use bytemuck::{Pod, Zeroable};
use std::fmt;

/// Maximum number of vertices per mesh.
pub const MAX_VERTICES: u32 = 1_000_000;

/// Maximum number of indices per mesh.
pub const MAX_INDICES: u32 = 3_000_000;

/// A single vertex with position, normal, and texture coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        }
    }
}

/// CPU-side mesh data: vertices and indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Errors that can occur when constructing or validating meshes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    TooManyVertices { count: u32, max: u32 },
    TooManyIndices { count: u32, max: u32 },
    InvalidIndex { index: u32, vertex_count: u32 },
    NonFinitePosition { vertex: u32 },
    NonFiniteNormal { vertex: u32 },
    NonFiniteUv { vertex: u32 },
}

impl fmt::Display for MeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyVertices { count, max } => write!(
                formatter,
                "mesh has {count} vertices, exceeding limit of {max}"
            ),
            Self::TooManyIndices { count, max } => write!(
                formatter,
                "mesh has {count} indices, exceeding limit of {max}"
            ),
            Self::InvalidIndex {
                index,
                vertex_count,
            } => write!(
                formatter,
                "index {index} references non-existent vertex (only {vertex_count} vertices)"
            ),
            Self::NonFinitePosition { vertex } => {
                write!(formatter, "vertex {vertex} has non-finite position")
            }
            Self::NonFiniteNormal { vertex } => {
                write!(formatter, "vertex {vertex} has non-finite normal")
            }
            Self::NonFiniteUv { vertex } => {
                write!(formatter, "vertex {vertex} has non-finite uv")
            }
        }
    }
}

impl std::error::Error for MeshError {}

impl MeshData {
    /// Create an empty mesh.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mesh from vertices and indices with validation.
    pub fn from_parts(vertices: Vec<Vertex>, indices: Vec<u32>) -> Result<Self, MeshError> {
        if vertices.len() > MAX_VERTICES as usize {
            return Err(MeshError::TooManyVertices {
                count: vertices.len() as u32,
                max: MAX_VERTICES,
            });
        }
        if indices.len() > MAX_INDICES as usize {
            return Err(MeshError::TooManyIndices {
                count: indices.len() as u32,
                max: MAX_INDICES,
            });
        }
        for &index in &indices {
            if index as usize >= vertices.len() {
                return Err(MeshError::InvalidIndex {
                    index,
                    vertex_count: vertices.len() as u32,
                });
            }
        }
        Ok(Self { vertices, indices })
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }

    /// Number of indices.
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    /// Number of triangles (indices / 3).
    pub fn triangle_count(&self) -> u32 {
        self.indices.len() as u32 / 3
    }

    /// Whether the mesh has indices.
    pub fn is_indexed(&self) -> bool {
        !self.indices.is_empty()
    }

    /// Validate that all vertices have finite positions and normals.
    pub fn validate_geometry(&self) -> Result<(), MeshError> {
        for (i, vertex) in self.vertices.iter().enumerate() {
            if !vertex.position[0].is_finite()
                || !vertex.position[1].is_finite()
                || !vertex.position[2].is_finite()
            {
                return Err(MeshError::NonFinitePosition { vertex: i as u32 });
            }
            if !vertex.normal[0].is_finite()
                || !vertex.normal[1].is_finite()
                || !vertex.normal[2].is_finite()
            {
                return Err(MeshError::NonFiniteNormal { vertex: i as u32 });
            }
            if !vertex.uv[0].is_finite() || !vertex.uv[1].is_finite() {
                return Err(MeshError::NonFiniteUv { vertex: i as u32 });
            }
        }
        Ok(())
    }

    /// Generate flat-shaded normals from triangle faces.
    pub fn compute_flat_normals(&mut self) {
        for vertex in &mut self.vertices {
            vertex.normal = [0.0, 0.0, 0.0];
        }
        for tri in self.indices.chunks_exact(3) {
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let p0 = self.vertices[i0].position;
            let p1 = self.vertices[i1].position;
            let p2 = self.vertices[i2].position;
            let edge1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let edge2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            // Cross product.
            let normal = [
                edge1[1] * edge2[2] - edge1[2] * edge2[1],
                edge1[2] * edge2[0] - edge1[0] * edge2[2],
                edge1[0] * edge2[1] - edge1[1] * edge2[0],
            ];
            let len_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
            if len_sq > 0.0 {
                let len = len_sq.sqrt();
                let normalized = [normal[0] / len, normal[1] / len, normal[2] / len];
                for i in [i0, i1, i2] {
                    self.vertices[i].normal[0] += normalized[0];
                    self.vertices[i].normal[1] += normalized[1];
                    self.vertices[i].normal[2] += normalized[2];
                }
            }
        }
        for vertex in &mut self.vertices {
            let n = vertex.normal;
            let len_sq = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
            if len_sq > 0.0 {
                let len = len_sq.sqrt();
                vertex.normal = [n[0] / len, n[1] / len, n[2] / len];
            }
        }
    }
}

/// A handle to a GPU-resident mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub u32);

/// A handle to a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);

/// Renderable mesh component: associates an entity with a mesh and material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderableMesh {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}

impl RenderableMesh {
    pub const fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self { mesh, material }
    }
}

/// A draw request produced during render extraction.
#[derive(Clone, Debug)]
pub struct DrawRequest {
    pub entity: u32,
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub model_matrix: [f32; 16],
}

/// A unit cube mesh (for testing and simple geometry).
pub fn unit_cube() -> MeshData {
    let vertices = vec![
        // Front face
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        // Back face
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 1.0],
        },
        // Top face
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
        },
        // Bottom face
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 0.0],
        },
        // Right face
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        // Left face
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
    ];

    let indices = vec![
        0, 1, 2, 0, 2, 3, // Front
        4, 6, 5, 4, 7, 6, // Back
        8, 9, 10, 8, 10, 11, // Top
        12, 14, 13, 12, 15, 14, // Bottom
        16, 17, 18, 16, 18, 19, // Right
        20, 22, 21, 20, 23, 22, // Left
    ];

    MeshData::from_parts(vertices, indices).expect("unit cube is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_cube_is_valid() {
        let cube = unit_cube();
        assert_eq!(cube.vertex_count(), 24);
        assert_eq!(cube.index_count(), 36);
        assert_eq!(cube.triangle_count(), 12);
        cube.validate_geometry()
            .expect("unit cube geometry is valid");
    }

    #[test]
    fn mesh_validation_rejects_invalid_index() {
        let result = MeshData::from_parts(vec![Vertex::default(); 4], vec![0, 1, 10]);
        assert!(matches!(
            result,
            Err(MeshError::InvalidIndex { index: 10, .. })
        ));
    }

    #[test]
    fn mesh_validation_rejects_non_finite_position() {
        let mut mesh = unit_cube();
        mesh.vertices[0].position = [f32::NAN, 0.0, 0.0];
        assert!(matches!(
            mesh.validate_geometry(),
            Err(MeshError::NonFinitePosition { vertex: 0 })
        ));
    }

    #[test]
    fn mesh_count_limits() {
        let big = vec![Vertex::default(); (MAX_VERTICES + 1) as usize];
        assert!(matches!(
            MeshData::from_parts(big, vec![]),
            Err(MeshError::TooManyVertices { .. })
        ));
    }

    #[test]
    fn renderable_mesh_constructs() {
        let r = RenderableMesh::new(MeshHandle(1), MaterialHandle(2));
        assert_eq!(r.mesh, MeshHandle(1));
        assert_eq!(r.material, MaterialHandle(2));
    }
}
