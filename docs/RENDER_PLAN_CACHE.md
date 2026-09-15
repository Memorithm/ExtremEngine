# EE-PERF-04: shared render plans and derived pass names

This slice follows the rejection/recovery contract in `RENDER_GRAPH.md` and the
extraction work in `RENDER_EXTRACTION.md`. It removes repeated plan-vector and
pass-name copies from actual `Engine::tick`, not from a parallel demonstration.

## Public contracts

`RenderGraph::compile()` still returns an independent `CompiledRenderGraph` with
an owned execution-order vector. `cached_plan()` keeps its original borrowed type.
The additive `compile_shared()` API returns `Arc<CompiledRenderGraph>`: cache hits
share the immutable snapshot, whereas actual topology changes clear the cached Arc.
The iterative compiler, traversal order, errors and cache invalidation are unchanged.
Allocation failure and reference-count overflow are not recovery guarantees.

`RenderPlanPreparation`, owned by `extrem_render`, retains a plan and ordered names.
It always compiles/validates the supplied CURRENT graph first. An invalid graph is
not replaced by an old cached success. It compares live Arc identity, not version
alone: a different same-version graph, even at the same address, refreshes names.
The previous Arc remains alive during comparison, preventing allocator-address reuse
from masquerading as the same snapshot. Graph clones share until an edit invalidates
one; no-op edits do not trigger a name rebuild. Every future name/topology mutation
API MUST invalidate the plan, not only increment a version.

`CompiledRenderGraph` fields remain public for compatibility. A caller can edit its
owned copy, or call Arc::make_mut on a shared result, without mutating the live graph's
cache. Arc::get_mut cannot grant mutable access while the graph shares that plan.
Old snapshots are historical values, not authorization to render a mutated graph.

`Engine` reuses one preparation owner and preserves `last_render_passes()` as a
borrowed string slice. `last_plan_preparation_stats()` describes the last successful
frame's pass count, whether names were rebuilt, and UTF-8 payload bytes copied on
that call. Zero copied bytes on a cache hit is NOT an allocation-count/RSS or
whole-frame zero-allocation claim. Shared ownership adds atomic reference-count
operations and can retain an older plan while the new graph is invalid.

Standalone preparation owners can `clear()` their retained snapshot. Engine does
not expose that operation, so last-successful-frame observations remain intact.
`release_render_scratch()` continues to release extraction scratch only. Returned
graph errors leave previous frame state and preparation observations unchanged.
Panics in user systems/backends or allocations are still outside that contract.

## Validation

Eight added render tests cover sharing, independent owned/COW mutation, no-op and
rejected edits, invalidation, old snapshot lifetime, graph cloning, equal-version
replacement, exact empty/duplicate/UTF-8 names, error/recovery and release. Four
engine tests cover the actual frame loop, repeated mutable accessor calls, same-version
replacement, rejection and repair, graph cloning and empty graphs. Existing 65,536-graph
order/error parity, version-wrap and 50,000-depth tests remain mandatory, as do all
prior input/stage/frame recovery and extraction/propagation tests.

```bash
cargo test -p extrem_render --all-targets --locked
cargo test -p extrem_engine --all-targets --locked
cargo test -p extrem_engine -p extrem_render --doc --locked
cargo clippy -p extrem_engine -p extrem_render --all-targets --locked -- -D warnings
EXTREM_BENCH_REVISION="$(git rev-parse HEAD)" \
  cargo run --release -p extrem_engine --example graph_frame_bench --locked
```

## Before/after protocol

Reference engine: `6a2d459b32275b3742e56e16bceb638cba995ff3`. The new workflow checks
out that exact reference as a separate worktree and overlays ONLY the same benchmark
example used by the candidate. Source-byte equality is checked; the harness hash,
binary hashes, baseline overlay status and actual execution environment are saved.
Both complete engine versions are compiled, not just a reimplementation of old
copying code. The reference's original lockfile is respected with `--locked`.

Each executable exercises 3/32/256/2,048 passes with either 0 or 512 static entities,
in two regimes: hot unchanged graphs, and replacement with different names but the
same topology version before EVERY tick. There are 3 warmup samples then 21 measured
samples, each containing 16 timed ticks. Every tick checks frame count, exact ordered
names and submitted command count outside timing. Delta is zero, so this is not a
physics simulation workload. The backend is NullRenderer: there are no GPU commands,
window, vsync, asset streaming, real meshes or displayable frames in this measurement.

Timing includes the complete Engine::tick: graph preparation, App stages, transform
propagation, extraction/sorting, NullRenderer callbacks, diagnostic writes and input
end-frame processing. Construction/replacement of the input graph, assertions and
printing are excluded. On replacement, the candidate retains the old shared plan
until preparation; its final release can occur inside timing, whereas baseline graph
cache destruction is part of the preceding replacement. Interpret that boundary
when discussing cold/replacement results, not as a total edit-and-render cost.

Processes run in baseline/candidate/candidate/baseline order (two paired campaigns).
They use independent Worlds and randomized ECS maps; process repetition and alternating
order reduce some drift, but do not constitute a controlled dedicated hardware study.
Raw samples report TOTAL nanoseconds for 16 ticks. Divide median/p95 totals by 16
for amortized per-tick time. The 21-sample nearest-rank p95 is the 20th sorted sample,
not a reliable individual-frame tail-latency or stutter qualification.

Report small/default-graph cases as well as deliberately oversized graph stress
cases, and ALL regressions. No wall-clock threshold gates shared-runner CI. A faster
headless tick is not game FPS, GPU throughput, or a promise for populated game scenes.
No result is asserted here before execution; exact-SHA evidence belongs in the PR.

## Next product checkpoint

After this bounded optimization, prioritize real world mesh/material rendering in
the existing GPU crate, with validated geometry/indices, depth and camera/model
transforms, a runnable scene, and separately qualified visual/GPU evidence. Do not
keep substituting isolated CPU optimizations for the missing rendering capability.
