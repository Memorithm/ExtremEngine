# ExtremEngine Architecture Overview

ExtremEngine is a modular Rust game-engine project under active construction. Its architecture aims for explicit safety boundaries, testability and deterministic semantics where order matters; it does not claim universal determinism or production completeness.

## Workspace structure

- `extrem_math`: `Vec3`, normalized rotation `Quat`, TRS `Transform`, `Mat4`, WebGPU depth projection.
- `extrem_ecs`: type-indexed component/resource storage and generational entities. Iteration order is unspecified by design.
- `extrem_scene`: validated Parent/Children graph, transform propagation, camera and versioned RON scene document.
- `extrem_editor`: command transactions, undo/redo and inspector snapshots.
- `extrem_assets`: validated virtual asset keys, typed handles and collision-safe path identity.
- `extrem_app`: startup/fixed/update/post-update/render stages, bounded fixed-step accumulation, frame-budget accounting, deterministic LOD and DRS controllers.
- `extrem_input`: platform-neutral keyboard/mouse state.
- `extrem_window`: `winit` host that feeds native events into `Input` before frame callbacks.
- `extrem_gpu`: WGPU context, presentable surface management and a validation-triangle presenter.
- `extrem_render`: backend-neutral render commands, null/CPU backends and persistent render graph.
- `extrem_animation`: validated skeleton/clip/pose primitives, CPU LBS palette generation and EEFP/VPAE research interfaces.
- `extrem_physics`: deliberately minimal gravity/ground/box reference simulation with numeric validation; not a general rigid-body solver.
- `extrem_science`: validated Euler/RK4 ODE helpers and reusable integration workspace.
- `extrem_audio`: audio command/backend contract plus null backend; production output is deferred.
- `extrem_engine`: integration facade, persistent graph, deterministic extraction ordering, visibility-aware command extraction, CPU frame-budget/DRS sampling and `WgpuRenderer` adapter for the low-level validation presenter.

## Current invariants

1. **Unsafe-code policy** — workspace lint policy forbids unsafe Rust in ExtremEngine source.
2. **Order discipline** — ECS hash-storage iteration is never itself treated as semantic order. Engine camera selection uses an explicit entity-ID tie-break and render extraction sorts by entity.
3. **Entity lifetime** — index+generation handles reject stale entities; slots retire before generation wraparound.
4. **Hierarchy integrity** — managed hierarchy APIs update both directions; validators detect cycles, dangling/reverse-edge inconsistencies and duplicates. Traversals use visited sets where corrupted graph input could otherwise loop.
5. **External data validation** — asset paths, scene documents, animation clips/poses and numerical simulation inputs are validated before committing state.
6. **Render graph persistence** — topology is retained by `Engine`; compilation is cached and invalidated only on graph mutation.
7. **GPU status is explicit** — surface capability lists are checked, and every WGPU 30 acquisition outcome is represented rather than treated as a successful texture.
8. **Adaptive research does not override correctness** — EEFP/VPAE contracts expose validation, verification and rollback boundaries. They are experimental extension points, not performance or novelty claims.

## GPU boundary

The present GPU milestone is intentionally narrow but real:

```text
Window-owned surface target
→ compatible adapter/device
→ WGSL shader
→ render pipeline
→ render pass
→ draw validation triangle
→ queue submit
→ present
```

It does not yet render ECS meshes/materials. Mesh buffers, textures, depth, lighting, PBR, shadows, skinning shaders and post-processing remain subsequent rendering work.

## Determinism scope

Stage execution and explicitly sorted engine extraction are deterministic for the same in-process state and inputs. This document does not claim cross-platform floating-point bit identity, deterministic external drivers, deterministic GPU scheduling or deterministic future parallel execution.
