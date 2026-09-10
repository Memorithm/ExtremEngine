# ExtremEngine Architecture Overview

ExtremEngine is a modular, cross-platform Rust game engine designed for safety, determinism, testability, and modern animation / graphics workflows.

## Modular Workspace Structure

- `extrem_math`: Core mathematical primitives (`Vec3`, `Quat`, `Transform`, `Mat4`).
- `extrem_ecs`: Generational entity-component-system storage and resource manager.
- `extrem_scene`: Scene graph hierarchy, spatial propagation, parent/child relationships, camera, and RON document serialization.
- `extrem_editor`: Command-pattern transaction engine (`CommandRecord`), undo/redo history, and inspector snapshot views.
- `extrem_assets`: Path-canonicalized asset server, handles, and state management.
- `extrem_app`: Lifecycle scheduler, deterministic fixed/variable execution stages, and debt-clamped time accumulators.
- `extrem_input`: Platform-neutral keyboard, mouse, and action edge state.
- `extrem_window`: `winit` event loop and platform window host.
- `extrem_gpu`: Isolated `wgpu` device context and surface target manager.
- `extrem_render`: Render graph compiler, topological pass scheduler, and CPU/Null/GPU renderer backends.
- `extrem_animation`: Skeletal hierarchy, animation clip sampling, pose blending, matrix palette generation for Linear Blend Skinning (LBS), and adaptive evaluation hooks.
- `extrem_physics`: Rigid body dynamics, fixed-step solver, and box colliders.
- `extrem_science`: Numerical ODE solvers (Euler, RK4) and simulation clock.
- `extrem_audio`: Spatial audio commands and null/platform audio backends.
- `extrem_engine`: High-level engine facade combining all subsystems into a unified runtime.

## Architectural Invariants

1. **Safety**: `#![forbid(unsafe_code)]` across the entire codebase (except isolated platform abstraction boundaries where explicitly required and audited).
2. **Determinism**: HashMap iteration is never assumed to be deterministic for simulation/rendering order. Sorting and explicit priorities are enforced.
3. **Generational Lifetime**: Entity handles contain index and generation to prevent stale handle access.
