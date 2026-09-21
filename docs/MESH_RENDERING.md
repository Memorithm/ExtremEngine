# Native indexed world-mesh rendering

ExtremEngine renders ECS geometry through the existing WGPU context/surface authority. The current path is an opaque indexed renderer with depth, vertex normals and one directional Lambert light. It is not a PBR renderer and does not claim production-game throughput.

## Geometry and normals

`MeshData::new(vertices, indices)` preserves the original `MeshVertex { position, color }` API and derives object-space normals from triangle winding. Face cross products are accumulated in f64 and normalized once; vertices referenced only by degenerate triangles receive a +Z fallback normal so degenerate legacy geometry remains accepted. Shared vertices therefore receive smooth area-weighted normals.

Use `MeshData::new_with_normals(vertices, normals, indices)` for authored hard edges or imported normals. Explicit normals must have the same count as vertices, be finite and nonzero; they are normalized once at construction. Normals add 12 bytes per uploaded vertex, so `payload_bytes()` now accounts for 36 vertex bytes plus 4 bytes per u32 index.

The vertex shader derives an inverse-transpose-equivalent normal transform from the model basis using cofactors. Its determinant sign preserves mirrored transforms. Singular or f32-overflowing model bases are rejected before GPU submission. Nonuniform scaling is supported within this TRS-oriented contract; arbitrary ill-conditioned matrices are not promised to behave as a general-purpose numerical linear algebra package.

## Directional light contract

`DirectionalLight` is an ECS component with `active`, `direction_to_light`, linear RGB `color`, nonnegative `intensity` and scalar `ambient`. Direction is world-space and points from the surface toward the light. The lowest active generational Entity ID wins deterministically. Higher-ID lights are currently ignored; there is no multi-light accumulation yet.

The GPU receives a 96-byte frame uniform: 64-byte view-projection matrix, direction/intensity vec4 and light-color/ambient vec4. Fragment shading is:

```text
diffuse = max(dot(normalize(N_world), normalize(L)), 0) * intensity
rgb_out = clamp(base_rgb * (ambient + light_rgb * diffuse), 0, 1)
```

`extrem_gpu::shade_lambert` is the CPU reference used by tests. If no active `DirectionalLight` exists, the renderer uses `MeshLight::default()` (`ambient=1`, `intensity=0`) so prior unlit vertex/tint colors remain unchanged. This fallback is intentional compatibility, not an implicit scene light.

## Data flow

`MeshInstance` points at immutable `Arc<MeshData>` geometry and an opaque linear RGBA tint. `Engine::with_mesh_renderer` installs the backend extraction hook into the validated frame loop. After application stages it reads the current transforms (`GlobalTransform` before local `Transform`), respects `Visibility(false)`, selects the directional light and sorts visible instances by full Entity ID. Invalid selected lighting, transforms or materials reject the mesh batch; partial/stale draws are not submitted.

The ordinary camera extractor still selects the lowest eligible active camera. A nonempty mesh frame without a camera fails explicitly. The same pipeline serves a window surface or readable offscreen target. It uses real vertex/index buffers, one distinct model/tint instance record per draw, `Depth32Float`, `draw_indexed`, and cached uploads by live geometry identity. Alpha must equal one.

```bash
cargo run -p extrem_engine --example mesh_scene --locked
cargo run -p extrem_engine --example mesh_scene --locked -- --headless mesh-cubes.ppm
cargo run -p extrem_engine --example mesh_qualification --locked -- mesh-depth.ppm
```

The cube example uses white base vertex colors, generated face normals and a warm directional light; visible face brightness is therefore produced by the implemented Lambert path rather than manually authored face shades.

## Errors and resource limits

`tick` still returns `RenderGraphError`. Mesh/GPU results are separate through `renderer().last_mesh_result()`. A successful simulation tick is not proof that a GPU frame was submitted. Missing/replaced backend extraction is `MeshError::MissingExtraction`; missing camera for a nonempty batch is `MissingCamera`; invalid/singular normal transforms and invalid lighting are explicit errors.

Accepted limits remain 1,000,000 vertices, 3,000,000 indices per mesh, 4,096 visible draws, 256 resident unique geometries, 64 MiB of current vertex/normal/index payload and 8,388,608 target pixels, additionally bounded by device texture limits. These are accepted-payload bounds, not total allocator, driver, overdraw or frame-time quotas.

Geometry caching is by live `Arc` identity, not content hash. After validation, draws whose object-space AABB is fully outside the camera frustum are omitted from encoding; intersecting bounds are kept. Surviving consecutive draws that share one uploaded `MeshData` may collapse into one instanced `draw_indexed` (see `docs/INSTANCING.md`). Indirect draws are not implemented. Models/materials are uploaded each frame for surviving draws. Host allocation failure and all forms of device loss are not universally recoverable.

`MeshFrameReport::draw_calls` is the caller batch size before frustum culling. `culled_draw_calls` counts fully exterior AABBs. `encoded_draw_calls` is the number of indexed draw commands after culling and consecutive instancing. None of these values is GPU time or FPS.

Nested WGPU scopes capture Validation, OutOfMemory and Internal errors across resource creation, upload, submission, resize and readback. Offscreen readback removes 256-byte row padding and uses bounded native waits. This native blocking API is not the future browser/WASM asynchronous readback contract.

## Qualification

CPU tests cover automatic and explicit normals, invalid normals, nonuniform and mirrored normal transforms, singular rejection, directional-light bounds, a numerical Lambert reference and conservative frustum AABB culling. The required Mesh Qualification workflow then executes WGPU on the actual selected adapter. A missing adapter is failure, not a skipped success.

The pixel executable checks generated +Z normals, CPU/GPU Lambert agreement within one UNORM channel, normal rotation from lit to ambient-only, depth order independence, current camera movement, visibility, shared geometry upload reuse, odd-width padded readback, zero-size suspend/resume, invalid-input preservation and missing-hook rejection. The three-cube scene exercises the same lighting path with perspective transforms.

Software Vulkan/llvmpipe evidence establishes rasterization correctness for that environment only. It is not physical-GPU throughput, FPS, energy, driver portability or interactive-window qualification.

## Frustum culling

`MeshData` stores an object-space AABB computed at construction. `extrem_gpu::Frustum::from_view_projection` extracts six inward planes from the column-major view-projection matrix with WebGPU depth in `[0, 1]`. World AABBs are formed by transforming the eight local corners; a draw is rejected only when that AABB is completely outside a plane. Non-finite matrices or plane normals fail closed as `MeshError::InvalidMatrix`. This is conservative: partially visible bounds still draw, and there is no occlusion culling, hierarchical Z, or GPU-driven culling.

CPU unit tests cover near/side/behind/far rejection, order-preserving retain, non-finite VP failure and translated AABB extrema. Pixel qualification remains required for raster correctness and is unchanged by this CPU filter.

## Remaining product work

Textures/samplers, UV0, consecutive instancing, conservative frustum AABB culling and fail-closed static GLB import (`extrem_gpu::import_static_glb`) are implemented. Next priorities are broader batching/sorting with an explicit order contract, embedded/base-color texture import from GLB, and hardware profiling. PBR/IBL, multiple lights, shadows, transparency, skinning, Meshopt/Draco/KTX2, LOD/DRS and render-graph-driven GPU pass execution remain separate increments. Each should preserve the explicit resource/error contracts and add executed evidence before performance claims.


## Static GLB import

`extrem_gpu::import_static_glb` decodes a glTF 2.0 binary (`.glb`) into one `ImportedStaticMesh` per supported `TRIANGLES` primitive. Geometry is validated through the existing `MeshData` / `TexturedGeometry` constructors. The importer accepts `POSITION`, optional `NORMAL`, optional `TEXCOORD_0`, optional `COLOR_0`, and opaque `baseColorFactor` when vertex colors are absent.

It rejects skins, morph targets, sparse accessors, required extensions, external buffer URIs, non-triangle modes, material textures, and transparent base-color alpha. Node transforms, animations, cameras and `.gltf` JSON+URI packages are out of scope. Mesh-local space is preserved; callers apply engine transforms separately. See unit tests in `gltf_static.rs` for executed evidence.
