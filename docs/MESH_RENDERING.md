# Native indexed world-mesh rendering

This product slice follows EE-PERF-04. It renders ECS geometry using the existing
GPU context and surface authority, not a hard-coded validation triangle. It is a
first opaque vertex-color pipeline, not a complete modern game renderer.

## Data flow and use

`MeshData::new` validates immutable indexed triangle-list geometry. Each vertex has
a position and linear RGB color. Share its Arc in `MeshInstance` ECS components;
attach the usual local Transform and optional Visibility. `Engine::with_mesh_renderer`
installs the MeshExtractor hook into the normal frame loop. After App stages it
extracts current transforms (GlobalTransform preferred), hides Visibility(false),
rejects invalid transforms/colors and sorts visible instances by generational ID.
Missing transforms are skipped and counted. No partial draw list survives an error.

The unchanged camera extractor supplies the lowest eligible active camera. A
nonempty mesh frame without a camera fails explicitly. Empty frames clear the target;
there is no fallback triangle. A GPU frame uses vertex/index buffers, one camera
uniform and one distinct 80-byte model/tint instance record per draw. The shader
computes camera * model * vertex. Depth32Float is cleared to one, written, and tested
with Less. Rendering is double-sided; colors are vertex colors times the per-instance
linear tint. Alpha must equal one: transparency is rejected rather than faked.

Use `WgpuMeshRenderer::new(MeshRenderer::headless(w, h)?)` or
`MeshRenderer::for_surface(Arc<Window>, w, h)?`, then
`Engine::with_mesh_renderer(backend, config)`. The old WgpuRenderer/WgpuPresenter
remain explicit validation utilities for compatibility, not the world mesh path.
No RenderCommand enum or dependency/lockfile change is required. The backend
extraction hook takes a read-only World and the concrete backend, avoiding an ECS
or renderer dependency cycle in the GPU crate. Geometry/GPU arrays remain portable
column-major values, and the engine translates its existing math types at the seam.

```bash
cargo run -p extrem_engine --example mesh_scene --locked
cargo run -p extrem_engine --example mesh_scene --locked -- --headless mesh-cubes.ppm
cargo run -p extrem_engine --example mesh_qualification --locked -- mesh-depth.ppm
```

The window example animates three instances sharing a cube, updates aspect/targets
on resize and uses elapsed time. Face shading is authored vertex color, not lighting.
On fatal error it prints the diagnostic, stops rendering and leaves the window open
for the user to close. The existing window host/frame pacing is not redesigned.

## Errors and limits

`tick` still returns its existing RenderGraphError; rejected graphs never run the
new extraction hook. This is not a transaction covering GPU errors after simulation.
Check `renderer().last_mesh_result()` as well: an Err is a mesh validation/GPU failure;
an Ok with submitted=false is surface unavailability/suspension, not a rendered
frame. FrameStats keeps the prior camera/translation command-count meaning; actual
mesh draw/triangle/upload counts live in MeshFrameReport. drawn_pixels remains zero
because rasterized GPU pixel counts are not instrumented.

Accepted limits: 1,000,000 vertices and 3,000,000 indices per mesh; 4,096 visible draws;
256 unique meshes and 64 MiB vertex/index payload per frame/cache; at most 8,388,608
target pixels and device maximum dimensions (initialization also caps each axis at
4096). These bounds exclude driver overhead and caller vectors already allocated
before validation. Geometry is cached by live Arc identity and unused cache entries
are evicted each rendered frame. Equal but separately allocated meshes are not
content-deduplicated. Models/materials are uploaded each frame into distinct slots.
There is one draw_indexed call per visible entity, not batching or instancing groups.

Zero-size resize suspends without configuring an invalid surface. Nonzero target
sizes are checked before resource changes. Lost/outdated surfaces are reconfigured
once using the existing SurfaceTarget handling; suboptimal frames are presented and
then reconfigured. Offscreen readback strips 256-byte row padding, rejects reads
before a submitted frame or after resize, and uses bounded native poll/callback waits.
It is not a browser async readback API. Validation scopes report native WGPU
validation errors; allocation failure, device loss and user callback panics are not
universally recovered. Finite matrices and their product are checked; this is not
a guarantee against every extreme per-vertex floating-point overflow.

## Qualification, not performance claims

CPU contracts exercise geometry/index/color/matrix/count/extent validation and ECS
extraction. The required Mesh Qualification workflow installs a software Vulkan
implementation, records the actual selected adapter/backend and executes WGPU. A
missing adapter or failed pixel comparison fails the job, never becomes a skipped
"passing" GPU test. Odd-sized 65x49 and 67x51 targets test padded readback. Checks
cover independent per-instance models/materials, foreground occlusion independent
of draw order, camera movement, visibility, shared geometry upload reuse, suspension,
resize/resume and rejected-input preservation. A separate executable renders three
perspective cubes from the actual Engine pipeline and exports a PPM image.

Only actual executed logs/artifacts establish these outcomes. Software-Vulkan pixel
correctness is not hardware GPU throughput/FPS or interactive-window qualification.
The normal Linux MSRV/stable, Windows/macOS compile/tests, security, documentation
and existing CPU performance gates remain mandatory on the final head.

## Remaining product work

Implement lighting/normals and PBR/material textures, validated glTF/GLB import,
mesh/asset streaming, skinning and conservative culling/batching. Wire render-graph
passes to actual GPU execution instead of treating its names as proof of scheduled
GPU work. Qualify window/surface recovery and performance on actual hardware before
claiming a production renderer. Preserve existing graph rejection/recovery and
negative benchmark results; a visible cube is not an AAA engine.
