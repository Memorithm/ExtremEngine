//! Fail-closed static GLB (glTF 2.0 binary) import into `MeshData` / `TexturedGeometry`.
//!
//! Supported subset (honest, not a full glTF loader):
//! - Binary `.glb` containers with a JSON chunk and an optional BIN chunk
//! - `TRIANGLES` primitives (mode 4 or omitted default)
//! - `POSITION` (required), optional `NORMAL`, optional `TEXCOORD_0`, optional `COLOR_0`
//! - Indexed or non-indexed primitives
//! - Material `pbrMetallicRoughness.baseColorFactor` as linear RGB vertex tint when `COLOR_0` is absent
//!
//! Explicitly rejected: `.gltf`+external URI buffers, skins/`JOINTS_0`/`WEIGHTS_0`, morph targets,
//! sparse accessors, required unknown extensions, Draco/Meshopt, non-triangle modes, textures,
//! animations, cameras, and scene-graph node transforms (primitives are imported in mesh-local space).
use crate::{
    MAX_MESH_INDICES, MAX_MESH_VERTICES, MeshData, MeshError, MeshVertex, TextureError,
    TexturedGeometry,
};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;

const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;
const MODE_TRIANGLES: u32 = 4;
const COMPONENT_BYTE: u32 = 5120;
const COMPONENT_UNSIGNED_BYTE: u32 = 5121;
const COMPONENT_SHORT: u32 = 5122;
const COMPONENT_UNSIGNED_SHORT: u32 = 5123;
const COMPONENT_UNSIGNED_INT: u32 = 5125;
const COMPONENT_FLOAT: u32 = 5126;
const MAX_GLB_BYTES: usize = 64 * 1024 * 1024;
const MAX_PRIMITIVES: usize = 256;

/// Errors produced while validating or decoding a static GLB payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GltfImportError {
    Truncated,
    InvalidMagic,
    UnsupportedVersion,
    ChunkLayout,
    Json(String),
    Unsupported(&'static str),
    MissingAttribute(&'static str),
    Accessor(String),
    Capacity,
    Mesh(String),
    Texture(String),
}

impl fmt::Display for GltfImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "GLB payload is truncated"),
            Self::InvalidMagic => write!(f, "GLB magic is not glTF"),
            Self::UnsupportedVersion => write!(f, "only glTF binary version 2 is accepted"),
            Self::ChunkLayout => write!(f, "GLB chunk layout is invalid"),
            Self::Json(message) => write!(f, "GLB JSON rejected: {message}"),
            Self::Unsupported(feature) => write!(f, "GLB feature not supported: {feature}"),
            Self::MissingAttribute(name) => {
                write!(f, "primitive is missing required attribute {name}")
            }
            Self::Accessor(message) => write!(f, "accessor rejected: {message}"),
            Self::Capacity => write!(f, "imported geometry exceeds engine capacity"),
            Self::Mesh(message) => write!(f, "mesh construction failed: {message}"),
            Self::Texture(message) => write!(f, "UV coupling failed: {message}"),
        }
    }
}

impl std::error::Error for GltfImportError {}

impl From<MeshError> for GltfImportError {
    fn from(error: MeshError) -> Self {
        Self::Mesh(error.to_string())
    }
}

impl From<TextureError> for GltfImportError {
    fn from(error: TextureError) -> Self {
        Self::Texture(error.to_string())
    }
}

/// One imported triangle-list primitive in mesh-local space.
#[derive(Debug)]
pub struct ImportedStaticMesh {
    pub mesh_index: usize,
    pub primitive_index: usize,
    pub name: Option<String>,
    pub geometry: ImportedGeometry,
}

/// Geometry produced by the static importer.
#[derive(Debug)]
pub enum ImportedGeometry {
    Untextured(Arc<MeshData>),
    Textured(Arc<TexturedGeometry>),
}

impl ImportedGeometry {
    pub fn mesh(&self) -> &Arc<MeshData> {
        match self {
            Self::Untextured(mesh) => mesh,
            Self::Textured(textured) => textured.mesh(),
        }
    }
}

/// Imports every supported `TRIANGLES` primitive from a GLB byte slice.
pub fn import_static_glb(bytes: &[u8]) -> Result<Vec<ImportedStaticMesh>, GltfImportError> {
    if bytes.len() > MAX_GLB_BYTES {
        return Err(GltfImportError::Capacity);
    }
    let (json_bytes, bin) = split_glb(bytes)?;
    let root: Value = serde_json::from_slice(&json_bytes)
        .map_err(|error| GltfImportError::Json(error.to_string()))?;
    validate_root(&root)?;
    let document = Document::parse(&root)?;
    document.import_all(bin.as_deref())
}

fn split_glb(bytes: &[u8]) -> Result<(Vec<u8>, Option<Vec<u8>>), GltfImportError> {
    if bytes.len() < 12 {
        return Err(GltfImportError::Truncated);
    }
    let magic = read_u32(bytes, 0)?;
    let version = read_u32(bytes, 4)?;
    let length = read_u32(bytes, 8)? as usize;
    if magic != GLB_MAGIC {
        return Err(GltfImportError::InvalidMagic);
    }
    if version != GLB_VERSION {
        return Err(GltfImportError::UnsupportedVersion);
    }
    if length != bytes.len() {
        return Err(GltfImportError::ChunkLayout);
    }

    let mut offset = 12usize;
    let mut json = None;
    let mut bin = None;
    while offset + 8 <= bytes.len() {
        let chunk_length = read_u32(bytes, offset)? as usize;
        let chunk_type = read_u32(bytes, offset + 4)?;
        offset += 8;
        let end = offset
            .checked_add(chunk_length)
            .ok_or(GltfImportError::ChunkLayout)?;
        if end > bytes.len() {
            return Err(GltfImportError::Truncated);
        }
        let data = bytes[offset..end].to_vec();
        offset = end;
        match chunk_type {
            CHUNK_JSON if json.is_none() => json = Some(data),
            CHUNK_BIN if bin.is_none() => bin = Some(data),
            CHUNK_JSON | CHUNK_BIN => return Err(GltfImportError::ChunkLayout),
            _ => return Err(GltfImportError::Unsupported("unknown GLB chunk type")),
        }
    }
    if offset != bytes.len() {
        return Err(GltfImportError::ChunkLayout);
    }
    let json = json.ok_or(GltfImportError::ChunkLayout)?;
    Ok((json, bin))
}

fn validate_root(root: &Value) -> Result<(), GltfImportError> {
    let asset = root
        .get("asset")
        .and_then(Value::as_object)
        .ok_or_else(|| GltfImportError::Json("missing asset".into()))?;
    let version = asset
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| GltfImportError::Json("missing asset.version".into()))?;
    if !version.starts_with("2.") && version != "2" {
        return Err(GltfImportError::UnsupportedVersion);
    }
    if let Some(required) = root.get("extensionsRequired").and_then(Value::as_array) {
        if !required.is_empty() {
            return Err(GltfImportError::Unsupported(
                "extensionsRequired is non-empty",
            ));
        }
    }
    if root
        .get("skins")
        .and_then(Value::as_array)
        .is_some_and(|s| !s.is_empty())
    {
        return Err(GltfImportError::Unsupported("skins"));
    }
    Ok(())
}

#[derive(Debug)]
struct Document {
    meshes: Vec<MeshNode>,
    accessors: Vec<Accessor>,
    buffer_views: Vec<BufferView>,
    buffers: Vec<BufferDesc>,
    materials: Vec<Material>,
}

#[derive(Debug)]
struct MeshNode {
    name: Option<String>,
    primitives: Vec<Primitive>,
}

#[derive(Debug)]
struct Primitive {
    attributes: Attributes,
    indices: Option<usize>,
    material: Option<usize>,
    mode: u32,
}

#[derive(Debug, Default)]
struct Attributes {
    position: Option<usize>,
    normal: Option<usize>,
    texcoord_0: Option<usize>,
    color_0: Option<usize>,
    joints_0: Option<usize>,
    weights_0: Option<usize>,
}

#[derive(Debug)]
struct Accessor {
    buffer_view: Option<usize>,
    byte_offset: usize,
    component_type: u32,
    count: usize,
    type_name: String,
    normalized: bool,
}

#[derive(Debug)]
struct BufferView {
    buffer: usize,
    byte_offset: usize,
    byte_length: usize,
    byte_stride: Option<usize>,
}

#[derive(Debug)]
struct BufferDesc {
    byte_length: usize,
    uri: Option<String>,
}

#[derive(Debug, Default)]
struct Material {
    base_color_factor: [f32; 4],
}

impl Document {
    fn parse(root: &Value) -> Result<Self, GltfImportError> {
        let meshes = root
            .get("meshes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(parse_mesh)
            .collect::<Result<Vec<_>, _>>()?;
        if meshes.is_empty() {
            return Err(GltfImportError::Json("no meshes".into()));
        }
        let accessors = root
            .get("accessors")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(parse_accessor)
            .collect::<Result<Vec<_>, _>>()?;
        let buffer_views = root
            .get("bufferViews")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(parse_buffer_view)
            .collect::<Result<Vec<_>, _>>()?;
        let buffers = root
            .get("buffers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(parse_buffer)
            .collect::<Result<Vec<_>, _>>()?;
        if buffers.is_empty() {
            return Err(GltfImportError::Json("no buffers".into()));
        }
        for buffer in &buffers {
            if buffer.uri.as_ref().is_some_and(|uri| !uri.is_empty()) {
                return Err(GltfImportError::Unsupported("external buffer URI"));
            }
        }
        let materials = root
            .get("materials")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(parse_material)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            meshes,
            accessors,
            buffer_views,
            buffers,
            materials,
        })
    }

    fn import_all(&self, bin: Option<&[u8]>) -> Result<Vec<ImportedStaticMesh>, GltfImportError> {
        let bin = match (bin, self.buffers.first()) {
            (Some(bytes), Some(desc)) => {
                if bytes.len() < desc.byte_length {
                    return Err(GltfImportError::Truncated);
                }
                &bytes[..desc.byte_length]
            }
            (None, Some(desc)) if desc.byte_length == 0 => &[][..],
            (None, _) => return Err(GltfImportError::Unsupported("missing BIN chunk")),
            _ => return Err(GltfImportError::Json("buffer[0] missing".into())),
        };
        if self.buffers.len() != 1 {
            return Err(GltfImportError::Unsupported(
                "only a single GLB BIN buffer is accepted",
            ));
        }

        let mut out = Vec::new();
        for (mesh_index, mesh) in self.meshes.iter().enumerate() {
            for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
                if out.len() >= MAX_PRIMITIVES {
                    return Err(GltfImportError::Capacity);
                }
                out.push(self.import_primitive(
                    bin,
                    mesh_index,
                    primitive_index,
                    mesh,
                    primitive,
                )?);
            }
        }
        if out.is_empty() {
            return Err(GltfImportError::Json("no supported primitives".into()));
        }
        Ok(out)
    }

    fn import_primitive(
        &self,
        bin: &[u8],
        mesh_index: usize,
        primitive_index: usize,
        mesh: &MeshNode,
        primitive: &Primitive,
    ) -> Result<ImportedStaticMesh, GltfImportError> {
        if primitive.mode != MODE_TRIANGLES {
            return Err(GltfImportError::Unsupported("non-TRIANGLES primitive mode"));
        }
        if primitive.attributes.joints_0.is_some() || primitive.attributes.weights_0.is_some() {
            return Err(GltfImportError::Unsupported("skinned primitive attributes"));
        }
        let position_index = primitive
            .attributes
            .position
            .ok_or(GltfImportError::MissingAttribute("POSITION"))?;
        let positions = self.read_vec3_f32(bin, position_index, "POSITION")?;
        let vertex_count = positions.len();
        if vertex_count == 0 || vertex_count > MAX_MESH_VERTICES {
            return Err(GltfImportError::Capacity);
        }

        let normals = match primitive.attributes.normal {
            Some(index) => Some(self.read_vec3_f32(bin, index, "NORMAL")?),
            None => None,
        };
        if let Some(normals) = &normals {
            if normals.len() != vertex_count {
                return Err(GltfImportError::Accessor(
                    "NORMAL count does not match POSITION".into(),
                ));
            }
        }

        let uv0 = match primitive.attributes.texcoord_0 {
            Some(index) => Some(self.read_vec2_f32(bin, index, "TEXCOORD_0")?),
            None => None,
        };
        if let Some(uv0) = &uv0 {
            if uv0.len() != vertex_count {
                return Err(GltfImportError::Accessor(
                    "TEXCOORD_0 count does not match POSITION".into(),
                ));
            }
        }

        let base_color = primitive
            .material
            .and_then(|index| self.materials.get(index))
            .map(|material| material.base_color_factor)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        if base_color[3] != 1.0 {
            return Err(GltfImportError::Unsupported(
                "transparent baseColorFactor alpha",
            ));
        }

        let colors = match primitive.attributes.color_0 {
            Some(index) => self.read_colors_rgb(bin, index, vertex_count)?,
            None => vec![[base_color[0], base_color[1], base_color[2]]; vertex_count],
        };

        let indices = match primitive.indices {
            Some(index) => self.read_indices(bin, index)?,
            None => {
                if !vertex_count.is_multiple_of(3) {
                    return Err(GltfImportError::Accessor(
                        "non-indexed vertex count is not a multiple of 3".into(),
                    ));
                }
                (0..vertex_count as u32).collect()
            }
        };
        if indices.len() > MAX_MESH_INDICES || indices.len() % 3 != 0 {
            return Err(GltfImportError::Capacity);
        }

        let vertices = positions
            .into_iter()
            .zip(colors)
            .map(|(position, color)| MeshVertex { position, color })
            .collect::<Vec<_>>();
        let mesh_data = match normals {
            Some(normals) => MeshData::new_with_normals(vertices, normals, indices)?,
            None => MeshData::new(vertices, indices)?,
        };
        let geometry = match uv0 {
            Some(uv0) => ImportedGeometry::Textured(TexturedGeometry::new(mesh_data, uv0)?),
            None => ImportedGeometry::Untextured(mesh_data),
        };
        Ok(ImportedStaticMesh {
            mesh_index,
            primitive_index,
            name: mesh.name.clone(),
            geometry,
        })
    }

    fn accessor(&self, index: usize, label: &str) -> Result<&Accessor, GltfImportError> {
        self.accessors.get(index).ok_or_else(|| {
            GltfImportError::Accessor(format!("{label} accessor {index} is missing"))
        })
    }

    fn view_slice<'a>(
        &self,
        bin: &'a [u8],
        accessor: &Accessor,
        label: &str,
    ) -> Result<(&'a [u8], usize), GltfImportError> {
        let view_index = accessor
            .buffer_view
            .ok_or(GltfImportError::Unsupported("accessor without bufferView"))?;
        let view = self.buffer_views.get(view_index).ok_or_else(|| {
            GltfImportError::Accessor(format!("{label} bufferView {view_index} missing"))
        })?;
        if view.buffer != 0 {
            return Err(GltfImportError::Unsupported("non-zero buffer index"));
        }
        let start =
            view.byte_offset
                .checked_add(accessor.byte_offset)
                .ok_or(GltfImportError::Accessor(format!(
                    "{label} offset overflow"
                )))?;
        let end = view
            .byte_offset
            .checked_add(view.byte_length)
            .ok_or(GltfImportError::Accessor(format!("{label} view overflow")))?;
        if end > bin.len() || start > end {
            return Err(GltfImportError::Truncated);
        }
        let stride = view
            .byte_stride
            .unwrap_or_else(|| accessor.element_stride().unwrap_or(0));
        if stride == 0 {
            return Err(GltfImportError::Accessor(format!(
                "{label} has invalid stride"
            )));
        }
        Ok((&bin[start..end], stride))
    }

    fn read_vec3_f32(
        &self,
        bin: &[u8],
        index: usize,
        label: &'static str,
    ) -> Result<Vec<[f32; 3]>, GltfImportError> {
        let accessor = self.accessor(index, label)?;
        if accessor.component_type != COMPONENT_FLOAT || accessor.type_name != "VEC3" {
            return Err(GltfImportError::Accessor(format!(
                "{label} must be FLOAT VEC3"
            )));
        }
        if accessor.normalized {
            return Err(GltfImportError::Accessor(format!(
                "{label} must not be normalized"
            )));
        }
        let (slice, stride) = self.view_slice(bin, accessor, label)?;
        let mut out = Vec::with_capacity(accessor.count);
        for i in 0..accessor.count {
            let offset = i
                .checked_mul(stride)
                .ok_or(GltfImportError::Accessor(format!(
                    "{label} stride overflow"
                )))?;
            let end = offset
                .checked_add(12)
                .ok_or(GltfImportError::Accessor(format!(
                    "{label} element overflow"
                )))?;
            if end > slice.len() {
                return Err(GltfImportError::Truncated);
            }
            out.push([
                read_f32(slice, offset)?,
                read_f32(slice, offset + 4)?,
                read_f32(slice, offset + 8)?,
            ]);
        }
        Ok(out)
    }

    fn read_vec2_f32(
        &self,
        bin: &[u8],
        index: usize,
        label: &'static str,
    ) -> Result<Vec<[f32; 2]>, GltfImportError> {
        let accessor = self.accessor(index, label)?;
        if accessor.component_type != COMPONENT_FLOAT || accessor.type_name != "VEC2" {
            return Err(GltfImportError::Accessor(format!(
                "{label} must be FLOAT VEC2"
            )));
        }
        let (slice, stride) = self.view_slice(bin, accessor, label)?;
        let mut out = Vec::with_capacity(accessor.count);
        for i in 0..accessor.count {
            let offset = i
                .checked_mul(stride)
                .ok_or(GltfImportError::Accessor(format!(
                    "{label} stride overflow"
                )))?;
            let end = offset
                .checked_add(8)
                .ok_or(GltfImportError::Accessor(format!(
                    "{label} element overflow"
                )))?;
            if end > slice.len() {
                return Err(GltfImportError::Truncated);
            }
            out.push([read_f32(slice, offset)?, read_f32(slice, offset + 4)?]);
        }
        Ok(out)
    }

    fn read_colors_rgb(
        &self,
        bin: &[u8],
        index: usize,
        expected: usize,
    ) -> Result<Vec<[f32; 3]>, GltfImportError> {
        let accessor = self.accessor(index, "COLOR_0")?;
        if accessor.count != expected {
            return Err(GltfImportError::Accessor(
                "COLOR_0 count does not match POSITION".into(),
            ));
        }
        let components = match accessor.type_name.as_str() {
            "VEC3" => 3usize,
            "VEC4" => 4usize,
            _ => {
                return Err(GltfImportError::Accessor(
                    "COLOR_0 must be VEC3 or VEC4".into(),
                ));
            }
        };
        let (slice, stride) = self.view_slice(bin, accessor, "COLOR_0")?;
        let mut out = Vec::with_capacity(accessor.count);
        for i in 0..accessor.count {
            let offset = i
                .checked_mul(stride)
                .ok_or_else(|| GltfImportError::Accessor("COLOR_0 stride overflow".into()))?;
            let rgb = match accessor.component_type {
                COMPONENT_FLOAT => {
                    let need = components * 4;
                    if offset + need > slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    [
                        read_f32(slice, offset)?,
                        read_f32(slice, offset + 4)?,
                        read_f32(slice, offset + 8)?,
                    ]
                }
                COMPONENT_UNSIGNED_BYTE if accessor.normalized => {
                    if offset + components > slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    [
                        slice[offset] as f32 / 255.0,
                        slice[offset + 1] as f32 / 255.0,
                        slice[offset + 2] as f32 / 255.0,
                    ]
                }
                COMPONENT_UNSIGNED_SHORT if accessor.normalized => {
                    let need = components * 2;
                    if offset + need > slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    [
                        read_u16(slice, offset)? as f32 / 65535.0,
                        read_u16(slice, offset + 2)? as f32 / 65535.0,
                        read_u16(slice, offset + 4)? as f32 / 65535.0,
                    ]
                }
                _ => {
                    return Err(GltfImportError::Unsupported("COLOR_0 component type"));
                }
            };
            if !rgb.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c)) {
                return Err(GltfImportError::Accessor(
                    "COLOR_0 values must be finite in [0, 1]".into(),
                ));
            }
            out.push(rgb);
        }
        Ok(out)
    }

    fn read_indices(&self, bin: &[u8], index: usize) -> Result<Vec<u32>, GltfImportError> {
        let accessor = self.accessor(index, "indices")?;
        if accessor.type_name != "SCALAR" {
            return Err(GltfImportError::Accessor("indices must be SCALAR".into()));
        }
        if accessor.normalized {
            return Err(GltfImportError::Accessor(
                "indices must not be normalized".into(),
            ));
        }
        let (slice, stride) = self.view_slice(bin, accessor, "indices")?;
        let mut out = Vec::with_capacity(accessor.count);
        for i in 0..accessor.count {
            let offset = i
                .checked_mul(stride)
                .ok_or_else(|| GltfImportError::Accessor("indices stride overflow".into()))?;
            let value = match accessor.component_type {
                COMPONENT_UNSIGNED_BYTE => {
                    if offset >= slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    u32::from(slice[offset])
                }
                COMPONENT_UNSIGNED_SHORT => {
                    if offset + 2 > slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    u32::from(read_u16(slice, offset)?)
                }
                COMPONENT_UNSIGNED_INT => {
                    if offset + 4 > slice.len() {
                        return Err(GltfImportError::Truncated);
                    }
                    read_u32(slice, offset)?
                }
                _ => {
                    return Err(GltfImportError::Unsupported("index component type"));
                }
            };
            out.push(value);
        }
        Ok(out)
    }
}

impl Accessor {
    fn element_stride(&self) -> Option<usize> {
        let components = match self.type_name.as_str() {
            "SCALAR" => 1usize,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let size = match self.component_type {
            COMPONENT_BYTE | COMPONENT_UNSIGNED_BYTE => 1usize,
            COMPONENT_SHORT | COMPONENT_UNSIGNED_SHORT => 2,
            COMPONENT_UNSIGNED_INT | COMPONENT_FLOAT => 4,
            _ => return None,
        };
        Some(components * size)
    }
}

fn parse_mesh(value: Value) -> Result<MeshNode, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("mesh must be an object".into()))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let primitives = object
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or_else(|| GltfImportError::Json("mesh.primitives missing".into()))?
        .iter()
        .cloned()
        .map(parse_primitive)
        .collect::<Result<Vec<_>, _>>()?;
    if primitives.is_empty() {
        return Err(GltfImportError::Json("mesh has no primitives".into()));
    }
    if object.get("weights").is_some() {
        return Err(GltfImportError::Unsupported("morph target weights"));
    }
    Ok(MeshNode { name, primitives })
}

fn parse_primitive(value: Value) -> Result<Primitive, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("primitive must be an object".into()))?;
    if object.get("targets").is_some() {
        return Err(GltfImportError::Unsupported("morph targets"));
    }
    let attributes_value = object
        .get("attributes")
        .and_then(Value::as_object)
        .ok_or_else(|| GltfImportError::Json("primitive.attributes missing".into()))?;
    let mut attributes = Attributes::default();
    for (key, value) in attributes_value {
        let index = value
            .as_u64()
            .ok_or_else(|| GltfImportError::Json(format!("attribute {key} index must be u64")))?
            as usize;
        match key.as_str() {
            "POSITION" => attributes.position = Some(index),
            "NORMAL" => attributes.normal = Some(index),
            "TEXCOORD_0" => attributes.texcoord_0 = Some(index),
            "COLOR_0" => attributes.color_0 = Some(index),
            "JOINTS_0" => attributes.joints_0 = Some(index),
            "WEIGHTS_0" => attributes.weights_0 = Some(index),
            "TEXCOORD_1" | "TANGENT" => {
                return Err(GltfImportError::Unsupported("advanced mesh attributes"));
            }
            _ => {
                return Err(GltfImportError::Unsupported("unknown primitive attribute"));
            }
        }
    }
    let indices = object
        .get("indices")
        .map(|value| {
            value
                .as_u64()
                .map(|v| v as usize)
                .ok_or_else(|| GltfImportError::Json("indices must be u64".into()))
        })
        .transpose()?;
    let material = object
        .get("material")
        .map(|value| {
            value
                .as_u64()
                .map(|v| v as usize)
                .ok_or_else(|| GltfImportError::Json("material must be u64".into()))
        })
        .transpose()?;
    let mode = object
        .get("mode")
        .map(|value| {
            value
                .as_u64()
                .map(|v| v as u32)
                .ok_or_else(|| GltfImportError::Json("mode must be u64".into()))
        })
        .transpose()?
        .unwrap_or(MODE_TRIANGLES);
    Ok(Primitive {
        attributes,
        indices,
        material,
        mode,
    })
}

fn parse_accessor(value: Value) -> Result<Accessor, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("accessor must be an object".into()))?;
    if object.get("sparse").is_some() {
        return Err(GltfImportError::Unsupported("sparse accessors"));
    }
    Ok(Accessor {
        buffer_view: object
            .get("bufferView")
            .map(|value| {
                value
                    .as_u64()
                    .map(|v| v as usize)
                    .ok_or_else(|| GltfImportError::Json("bufferView must be u64".into()))
            })
            .transpose()?,
        byte_offset: object
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        component_type: object
            .get("componentType")
            .and_then(Value::as_u64)
            .ok_or_else(|| GltfImportError::Json("componentType missing".into()))?
            as u32,
        count: object
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| GltfImportError::Json("count missing".into()))? as usize,
        type_name: object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| GltfImportError::Json("type missing".into()))?
            .to_owned(),
        normalized: object
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_buffer_view(value: Value) -> Result<BufferView, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("bufferView must be an object".into()))?;
    Ok(BufferView {
        buffer: object
            .get("buffer")
            .and_then(Value::as_u64)
            .ok_or_else(|| GltfImportError::Json("bufferView.buffer missing".into()))?
            as usize,
        byte_offset: object
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        byte_length: object
            .get("byteLength")
            .and_then(Value::as_u64)
            .ok_or_else(|| GltfImportError::Json("byteLength missing".into()))?
            as usize,
        byte_stride: object
            .get("byteStride")
            .map(|value| {
                value
                    .as_u64()
                    .map(|v| v as usize)
                    .ok_or_else(|| GltfImportError::Json("byteStride must be u64".into()))
            })
            .transpose()?,
    })
}

fn parse_buffer(value: Value) -> Result<BufferDesc, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("buffer must be an object".into()))?;
    Ok(BufferDesc {
        byte_length: object
            .get("byteLength")
            .and_then(Value::as_u64)
            .ok_or_else(|| GltfImportError::Json("buffer.byteLength missing".into()))?
            as usize,
        uri: object.get("uri").and_then(Value::as_str).map(str::to_owned),
    })
}

fn parse_material(value: Value) -> Result<Material, GltfImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| GltfImportError::Json("material must be an object".into()))?;
    if object.get("extensions").is_some() {
        return Err(GltfImportError::Unsupported("material extensions"));
    }
    let mut material = Material {
        base_color_factor: [1.0, 1.0, 1.0, 1.0],
    };
    if let Some(pbr) = object
        .get("pbrMetallicRoughness")
        .and_then(Value::as_object)
    {
        if pbr.get("baseColorTexture").is_some() || pbr.get("metallicRoughnessTexture").is_some() {
            return Err(GltfImportError::Unsupported("material textures"));
        }
        if let Some(factor) = pbr.get("baseColorFactor").and_then(Value::as_array) {
            if factor.len() != 4 {
                return Err(GltfImportError::Json(
                    "baseColorFactor must have 4 components".into(),
                ));
            }
            for (dst, src) in material.base_color_factor.iter_mut().zip(factor) {
                *dst = src.as_f64().ok_or_else(|| {
                    GltfImportError::Json("baseColorFactor must be numeric".into())
                })? as f32;
            }
            if !material
                .base_color_factor
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            {
                return Err(GltfImportError::Json(
                    "baseColorFactor must be finite in [0, 1]".into(),
                ));
            }
        }
    }
    Ok(material)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, GltfImportError> {
    let array: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(GltfImportError::Truncated)?
        .try_into()
        .map_err(|_| GltfImportError::Truncated)?;
    Ok(u16::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, GltfImportError> {
    let array: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(GltfImportError::Truncated)?
        .try_into()
        .map_err(|_| GltfImportError::Truncated)?;
    Ok(u32::from_le_bytes(array))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, GltfImportError> {
    let array: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(GltfImportError::Truncated)?
        .try_into()
        .map_err(|_| GltfImportError::Truncated)?;
    Ok(f32::from_le_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::{GltfImportError, ImportedGeometry, import_static_glb};
    use std::io::Write;

    fn pad4(len: usize) -> usize {
        (4 - (len % 4)) % 4
    }

    fn build_glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json_bytes = json.as_bytes().to_vec();
        json_bytes.extend(std::iter::repeat_n(b' ', pad4(json_bytes.len())));
        let mut bin_bytes = bin.to_vec();
        bin_bytes.extend(std::iter::repeat_n(0u8, pad4(bin_bytes.len())));
        let total = 12 + 8 + json_bytes.len() + 8 + bin_bytes.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&json_bytes);
        out.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&bin_bytes);
        out
    }

    fn triangle_glb() -> Vec<u8> {
        // positions (3*vec3) + indices (3*u16)
        let mut bin = Vec::new();
        for position in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for component in position {
                bin.extend_from_slice(&component.to_le_bytes());
            }
        }
        for index in [0u16, 1, 2] {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        let json = r#"{
          "asset":{"version":"2.0"},
          "meshes":[{"name":"tri","primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
          "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}
          ],
          "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":6}
          ],
          "buffers":[{"byteLength":42}],
          "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.4,0.6,1.0]}}]
        }"#;
        build_glb(json, &bin)
    }

    #[test]
    fn imports_indexed_triangle_with_base_color() {
        let imported = import_static_glb(&triangle_glb()).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name.as_deref(), Some("tri"));
        let mesh = imported[0].geometry.mesh();
        assert_eq!(mesh.indices(), &[0, 1, 2]);
        assert_eq!(mesh.vertices()[0].color, [0.2, 0.4, 0.6]);
        assert!(mesh.normals()[0][2] > 0.99);
        assert!(matches!(
            imported[0].geometry,
            ImportedGeometry::Untextured(_)
        ));
    }

    #[test]
    fn imports_uv0_as_textured_geometry() {
        let mut bin = Vec::new();
        for position in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for component in position {
                bin.extend_from_slice(&component.to_le_bytes());
            }
        }
        for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for component in uv {
                bin.extend_from_slice(&component.to_le_bytes());
            }
        }
        for index in [0u16, 1, 2] {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        let json = r#"{
          "asset":{"version":"2.0"},
          "meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2}]}],
          "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"},
            {"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}
          ],
          "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":24},
            {"buffer":0,"byteOffset":60,"byteLength":6}
          ],
          "buffers":[{"byteLength":66}]
        }"#;
        let imported = import_static_glb(&build_glb(json, &bin)).unwrap();
        match &imported[0].geometry {
            ImportedGeometry::Textured(textured) => {
                assert_eq!(textured.uv0()[1], [1.0, 0.0]);
            }
            ImportedGeometry::Untextured(_) => panic!("expected textured"),
        }
    }

    #[test]
    fn rejects_skins_and_bad_magic() {
        let json = r#"{
          "asset":{"version":"2.0"},
          "skins":[{"joints":[0]}],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
          "bufferViews":[{"buffer":0,"byteLength":36}],
          "buffers":[{"byteLength":36}]
        }"#;
        let mut bin = Vec::new();
        let _ = writeln!(&mut bin);
        bin.clear();
        for _ in 0..9 {
            bin.extend_from_slice(&0f32.to_le_bytes());
        }
        assert_eq!(
            import_static_glb(&build_glb(json, &bin)).unwrap_err(),
            GltfImportError::Unsupported("skins")
        );
        assert_eq!(
            import_static_glb(b"nota glb file!!!!").unwrap_err(),
            GltfImportError::InvalidMagic
        );
    }

    #[test]
    fn rejects_required_extensions_and_external_uri() {
        let json = r#"{
          "asset":{"version":"2.0"},
          "extensionsRequired":["KHR_draco_mesh_compression"],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
          "bufferViews":[{"buffer":0,"byteLength":36}],
          "buffers":[{"byteLength":36}]
        }"#;
        let bin = vec![0u8; 36];
        assert!(matches!(
            import_static_glb(&build_glb(json, &bin)).unwrap_err(),
            GltfImportError::Unsupported("extensionsRequired is non-empty")
        ));

        let json = r#"{
          "asset":{"version":"2.0"},
          "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
          "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}
          ],
          "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":6}
          ],
          "buffers":[{"byteLength":42,"uri":"data.bin"}]
        }"#;
        assert!(matches!(
            import_static_glb(&build_glb(json, &bin)).unwrap_err(),
            GltfImportError::Unsupported("external buffer URI")
        ));
    }
}
