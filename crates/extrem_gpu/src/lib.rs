//! GPU context/surface authority and native indexed world-mesh rendering.
mod context;
pub use context::*;
mod mesh_data;
pub use mesh_data::{
    MAX_FRAME_DRAWS, MAX_GEOMETRY_BYTES, MAX_MESH_INDICES, MAX_MESH_VERTICES, MAX_RESIDENT_MESHES,
    MAX_TARGET_PIXELS, MeshData, MeshDraw, MeshError, MeshVertex, validate_extent, validate_frame,
    validate_matrix,
};
mod mesh_renderer;
pub use mesh_renderer::{MeshFrameReport, MeshRenderer};
mod mesh_safety;
