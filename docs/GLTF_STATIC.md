# Static GLB import

`extrem_gpu::import_static_glb` is a fail-closed decoder for glTF 2.0 binary containers.

## Claims

- Imports `TRIANGLES` primitives into validated `MeshData`, with optional `TexturedGeometry` when `TEXCOORD_0` is present.
- Uses authored `NORMAL` when provided; otherwise derives area-weighted normals via `MeshData::new`.
- Applies opaque `pbrMetallicRoughness.baseColorFactor` as vertex RGB when `COLOR_0` is absent.
- Rejects skins, morphs, sparse accessors, required extensions, external URIs, textures, non-triangle modes and transparent base-color alpha.

## Non-claims

- Not a full glTF 2.0 implementation.
- Does not load `.gltf` with external buffers or embedded data-URI images.
- Does not apply node/scene transforms, animations, cameras, skins or PBR shading.
- Does not claim Meshopt/Draco/KTX2 support.

Evidence: unit tests in `crates/extrem_gpu/src/gltf_static.rs`.
