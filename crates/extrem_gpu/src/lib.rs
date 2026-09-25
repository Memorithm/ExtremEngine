//! GPU context/surface authority and native indexed world-mesh rendering.
mod context;
pub use context::*;
mod frustum;
pub use frustum::{Aabb, Frustum, retain_draws_in_frustum, transform_aabb};
mod mesh_data;
pub use mesh_data::{
    MAX_FRAME_DRAWS, MAX_GEOMETRY_BYTES, MAX_LIGHT_INTENSITY, MAX_MESH_INDICES, MAX_MESH_VERTICES,
    MAX_RESIDENT_MESHES, MAX_SHININESS, MAX_TARGET_PIXELS, MeshData, MeshDraw, MeshError,
    MeshLight, MeshVertex, shade_blinn_phong, shade_lambert, transform_normal_reference,
    validate_extent, validate_frame, validate_lit_frame, validate_matrix,
    validate_normal_transform,
};
mod mesh_renderer;
pub use mesh_renderer::{MeshFrameReport, MeshRenderer};
mod mesh_safety;
mod mesh_uv;
pub use mesh_uv::TexturedGeometry;
mod texture;
pub use texture::{
    MAX_TEXTURE_BYTES, MAX_TEXTURE_DIMENSION, MAX_TEXTURE_PIXELS, MeshMaterial, TextureColorSpace,
    TextureData, TextureError,
};
mod gltf_static;
pub use gltf_static::{GltfImportError, ImportedGeometry, ImportedStaticMesh, import_static_glb};
