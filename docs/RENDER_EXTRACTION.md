# EE-PERF-03: reusable render extraction

This extends the EE-PERF-01/02 programme in `PERFORMANCE.md`. The old command
extraction body from `Engine::tick` at
`9fe9b2a13ec4b4a0df19e8f3ddcafdbeff06a0da` is retained in
`crates/extrem_engine/tests/support/legacy_extraction.rs`, parameterized only by
World, viewport aspect and backend. It is not part of the product library.

## Implementation and observable contract

The old path constructs a GlobalTransform entity HashSet and a vector of
`(Entity, RenderCommand)` every frame. It also builds a camera matrix for every
eligible camera before selecting the lowest entity ID.

`RenderExtractor` instead retains a compact `(Entity, Vec3)` vector, reads global
membership directly from the existing ECS storage and selects the camera before
computing its view-projection matrix. Entity keys are unique within each component
map, and local fallback excludes every global entity. An in-place unstable sort
therefore preserves the exact old order because there are no equal keys to reorder.
No external entity index controls an allocation. The existing randomized component
hash maps and full generational identifiers are unchanged.

The actual `Engine::tick` calls this extractor, owning one workspace per engine.
`last_extraction_stats()` reports candidate cameras, the selected ID, actual matrix
constructions, translation-command count and retained entry capacity.
`release_render_scratch()` explicitly drops extraction scratch after a large scene.
It does not release propagation, backend or render-graph allocations and does not
change previous-frame statistics. Standalone callers use `release_memory()`.

Every call rereads the World. There is no cached topology, visibility decision,
camera result or command buffer. Switching worlds with identical world-local IDs,
removing components and recycling an entity slot cannot reuse a previous command.

The following existing semantics are deliberately preserved:

- Select the lowest active camera with a global OR local transform; prefer global.
- Submit the camera first, then exactly one translation command per global/local
  union member in ascending generational Entity order. Global-only nodes count.
- Do not apply Visibility filtering: that requires a separate rendering contract.
- Preserve numeric bit patterns/fallback behavior; no new numeric sanitizer.
- Do not begin/end a frame inside the extractor: Engine retains that lifecycle.

This is not mesh/material rendering, culling, instancing, GPU upload or a full-frame
optimization claim. The mutable render graph can still fail in the existing tick
path, which remains a separately tracked correctness increment. Pass-name strings
and the graph's owned compiled-plan clone also remain outside this slice.

## Validation and reproduction

```bash
cargo test -p extrem_engine --test extraction_contract --locked
cargo test -p extrem_engine --doc --locked
cargo clippy -p extrem_engine --all-targets --locked -- -D warnings
EXTREM_BENCH_REVISION="$(git rev-parse HEAD)" \
  cargo run --release -p extrem_engine --example extraction_bench --locked
```

Ten new tests compare every command, entity, order and matrix/translation bit with
the old implementation. They include all 256 local/global presence patterns for
four entities checked against an independent oracle; inactive/missing cameras;
2,048 eligible cameras with exactly one matrix construction; global precedence;
visibility compatibility; slot reuse; signed zero/non-finite values; scratch release
and world replacement; and two tests exercising the actual Engine runtime.
The rustdoc example is executed, not just rendered into HTML.

The paired benchmark measures four profiles: all local+global components,
local-only, mixed presence (including empty entities), and all entities as cameras.
Each profile uses 32, 512, 2,048 and 20,000 entities, three warmups and 21 samples per
legacy/fresh/reused implementation. Order rotates on the same World. Live inputs
change each sample. All command bits are compared after every measurement.

Timed regions include camera selection/matrix construction, scratch management,
translation extraction/sorting and submission into a preallocated recording backend.
Scene construction, input edits, reference generation, output clearing, comparison,
printing, begin/end-frame callbacks, graph compilation, propagation and rendering
are outside timing. The fresh variant's scratch destruction is inside timing.
The reused variant is the one wired into Engine. Counts are not elapsed times.

Raw nanosecond samples, medians and nearest-rank p95 (20th of 21 sorted observations)
are uploaded with the exact executed SHA, toolchain, OS, CPU, memory and RUSTFLAGS.
Record every regression; a small p95 sample is not a tail-latency qualification.
A shared CI CPU microbenchmark cannot establish universal gains or game FPS.
No wall-clock threshold is a CI gate. Existing security, MSRV and platform checks
are retained; the new workflow additionally checks runtime parity and doctests.

## Next checkpoints

1. Repair the publicly mutable render graph's panic/error contract independently,
   including the frame lifecycle on a failed compile and recovery after repair.
2. Measure render-pass-name and compiled-plan copies, checking graph replacement
   and version invalidation before attempting a cache optimization.
3. Implement actual world mesh rendering in the existing GPU authority, followed
   by measured culling/batching/instancing. Do not confuse these markers with a
   completed game renderer or use LOD/DRS to hide correctness defects.

No benchmark result is asserted by this document before execution. PR evidence
records actual runs, negative results and validated merge checkpoints.
