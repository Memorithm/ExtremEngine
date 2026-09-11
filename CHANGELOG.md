# Changelog

## Unreleased

- Wired `extrem_app::{frame,lod,quality}` through the engine facade and measure CPU frame budget + DRS on every `Engine::tick`.
- Skip hidden (`Visibility(false)`) entities during render extraction.
- `Scene::prune_roots` drops stale root handles; scene documents ignore despawned roots.
- `Scene::spawn_child` now returns `HierarchyError` instead of a misleading `EntityNotFound`.
- Editor rename works on entities that have no `Name` yet; non-finite translation deltas are rejected.
- `World::remove_resource` added.
- `Mat4::perspective` sanitizes degenerate FOV/aspect/near/far.
- Re-export animation, science and web crates from `extrem_engine`.
- Added Apache-2.0 license file to match the declared dual license.
- Documented that GitHub default branch `agent/initial-engine` lags `main`.

## 0.1.0 - initial engine kernel

- Added a modular Rust workspace for ExtremEngine.
- Added typed entities, components, resources and lifecycle operations.
- Added startup, fixed-update, update, post-update and render schedules.
- Added fixed timestep configuration with a per-frame step cap.
- Added vector math, transforms and parent/child scene propagation.
- Added a replaceable render backend contract and a headless renderer.
- Added numerical simulation primitives for future SciRust/SciRS2 integration.
- Added typed asset handles, path normalization and in-memory asset registries.
- Added RON scene documents with hierarchy round-tripping and runtime instantiation.
- Added platform-neutral keyboard and mouse input state.
- Added camera projections and a dependency-checked render graph.
- Added a minimal gravity/ground/box physics reference solver.
- Added audio command/backend boundaries and a null audio backend.
- Added editor inspection commands with selection and undo support.
- Added RK4 numerical integration alongside Euler integration.
- Added a native window host and redraw-driven event loop abstraction.
- Added headless `wgpu` device discovery and re-exported GPU/window APIs from the engine facade.
- Added workspace tests, a runnable sandbox and GitHub Actions CI.
