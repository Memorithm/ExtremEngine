# Render graph rejection and frame recovery

The correctness increment following EE-PERF-03 made the existing frame loop
fallible. EE-PERF-04 subsequently added shared snapshots and derived-name reuse;
see `RENDER_PLAN_CACHE.md`. Neither creates another renderer or changes GPU ownership.

## API migration (pre-1.0 source change)

`Engine::tick(delta)` returns `Result<UpdateReport, RenderGraphError>` instead
of `UpdateReport`. `Engine::run_for(n)` returns
`Result<Vec<UpdateReport>, RenderGraphError>`. The error is the existing public
render-crate type, re-exported from `extrem_engine`; no placeholder error hierarchy
is added. Existing callers must propagate with `?` or explicitly handle failure.

```rust
use extrem_engine::{Engine, RenderGraphError};

fn run() -> Result<(), RenderGraphError> {
    let mut engine = Engine::new();
    let report = engine.tick(1.0 / 60.0)?;
    assert_eq!(report.frame, 1);
    let reports = engine.run_for(2)?;
    assert_eq!(reports.len(), 2);
    Ok(())
}
```

The sandbox example propagates errors instead of adding another `expect` around
tick. Existing runtime tests consume the Result and retain all prior assertions.
Executable rustdoc covers successful execution and repair/retry.

## Failure boundary

Before the correction, tick advanced the app and called `begin_frame` before
`compile().expect(...)`. A cycle could therefore unwind after updating the world,
advancing time and opening a backend frame without ending it.

Graph validation now precedes all application stages and renderer callbacks. A
returned graph error leaves world/components/resources, startup execution, fixed-step
debt, time/frame counters, input transitions, extraction scratch, backend state and
all last-successful-frame diagnostics unchanged. The attempted delta is not consumed.
A caller can correct the graph and retry without double-running simulation or
leaving an unmatched backend frame. Previous diagnostics remain last-successful
observations, not a claim that the rejected frame rendered. Shared-plan preparation
always validates the current graph before reusing the previous name snapshot.

`run_for` stops on the first error; this is not a rollback transaction over any
previously successful frames. `run_for(0)` is a no-op even for an invalid graph.
Use `render_graph_mut().compile()` to validate without requesting a frame.

Only returned graph errors are covered. User-system panics, renderer panics,
allocation failure, device loss and unrelated physics/scene errors are not
intercepted or rolled back by this API. The constant default graph still uses
checked construction assertions for pass IDs created immediately above; they
are not driven by caller input. No universal panic-free-engine claim is made.

## Compilation and in-place repair

The compiler uses explicit `(pass, next-dependency)` DFS frames rather than
recursive function calls. It preserves ascending root traversal, sorted adjacency,
dependency-before-consumer output and the same first cycle/missing-pass error.
A rejected compile never publishes a partial plan. Traversal work remains linear
in vertices/edges; heap scratch replaces call-stack depth. This is stack safety,
not a graph-wide resource quota.

`remove_dependency(pass, dependency)` returns `Ok(true)` only when it removes an
edge, invalidating the cache and advancing the topology version. An absent edge
between valid IDs returns `Ok(false)` without cache/version changes. An out-of-range
ID returns `MissingPass` without mutation. Pass IDs remain graph-local indices;
equal in-range IDs from different graphs are not distinguishable.

`compile()` still returns an owned plan copy. `compile_shared()` returns a shared
immutable snapshot of that same plan; the engine uses this additive API and reuses
names while the live plan identity is unchanged. Replacing a graph with a different
one of the same version cannot reuse the old engine name snapshot. Old shared
plans remain historical values when the current graph is edited. Tests cover
version wrap, cloning, external copy-on-write and corruption recovery.

## Validation

```bash
cargo test -p extrem_render --all-targets --locked
cargo test -p extrem_engine --test graph_failure_regressions --locked
cargo test -p extrem_engine --test graph_recovery_contract --locked
cargo test -p extrem_engine -p extrem_render --doc --locked
cargo run -p extrem_engine --example sandbox --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Three reproductions compile against the old tick API as well as the corrected one.
Five engine recovery tests check explicit errors, all stage counters, pending input,
world/time/diagnostics, repeated rejection, repair/retry, equal-version graph
replacement, batching and balanced backend callbacks. Seven graph tests include
all 65,536 directed four-node graphs (including self-loops) compared against the
frozen recursive traversal for exact order/first error, plus a 50,000-pass chain
and cycle on a thread requesting a 128 KiB stack. The legacy oracle runs only on
small fixtures; no old recursive deep traversal is executed in these tests.
Additional shared-plan/runtime tests are documented in `RENDER_PLAN_CACHE.md`.

Linux MSRV/stable run the tests and doctests; native Windows/macOS execute the
headless engine/render tests in addition to their existing editor tests. Existing
security, formatting, lint and benchmark gates remain. Listing commands here does
not assert that a particular future commit has passed them.

## Remaining product work

EE-PERF-04 addresses compiled-plan/pass-name copies with a separate before/after
protocol. Next advance actual world mesh/material rendering in the existing GPU
crate, with CPU contract tests and separately recorded visual/GPU evidence.
