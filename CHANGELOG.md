# Changelog

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
- Added deterministic rigid-body physics, gravity and ground contacts.
- Added audio command/backend boundaries and a null audio backend.
- Added editor inspection commands with selection and undo support.
- Added RK4 numerical integration alongside Euler integration.
- Added a native window host and redraw-driven event loop abstraction.
- Added headless `wgpu` device discovery and re-exported GPU/window APIs from the engine facade.
- Added workspace tests, a runnable sandbox and GitHub Actions CI.
