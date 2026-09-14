# ExtremEngine performance programme

Performance work follows measured bottlenecks, preserves semantics and keeps
portable CPU contracts separate from hardware GPU qualification. It is not an
FPS target or a claim that this prototype is already a production game engine.

## EE-PERF-01: hierarchy validation

Baseline: `extrem_scene::validate_hierarchy` at
`9dc2a79020f3bd000e985db807cbc474942e52d5`. Its ancestor walks repeat for each node;
a chain of N nodes follows N(N-1)/2 parent steps. For a wide parent, repeated
`Children::contains` searches also perform quadratic work. The frozen baseline
is retained in `crates/extrem_scene/tests/support/legacy_hierarchy.rs`, compiled
only into tests/examples, not into the production library.

The replacement builds a generational-entity parent index, validates reverse
edges with a single child set, and uses Unseen/Active/Finished states to visit
each parent link once during cycle discovery and once during completion. It
still rejects dangling generations, self-parenting, cycles, duplicate children,
and missing or disagreeing reverse links. Validation never modifies the World.
The first reported defect remains unspecified when multiple defects coexist.

The compatibility function retains its signature. `HierarchyValidator` is an
optional reusable scratch owner. Every call clears and rebuilds all semantic
state: allocation reuse is NOT a cached validation result. It can be reused
after an error, a mutation, or a different World. `release_memory()` drops its
retained collections; it does not promise an operating-system RSS reduction.

`HierarchyValidationStats` reports actual parent/child link counts and discovery
steps. These counts are not elapsed times. Expected graph work is linear under
the standard randomized HashMap assumptions. Total work also includes the
capacities scanned in the ECS component maps and retained scratch maps; after a
large world followed by a small one, release scratch memory when appropriate.
No allocation is indexed by an external entity's raw u32 index. The default
randomized hasher, generational checks and all hierarchy validation remain.

### Reproduction and acceptance

```bash
cargo test -p extrem_scene --all-targets --locked
cargo clippy -p extrem_scene --all-targets --locked -- -D warnings
EXTREM_BENCH_REVISION="$(git rev-parse HEAD)" \
  cargo run --release -p extrem_scene --example hierarchy_bench --locked
```

Eleven tests include all 625 four-node parent graphs checked against both the
frozen validator and an independent leaf-removal oracle. Deep and wide 20,000-node
fixtures assert exactly one discovery step per parent link without recursion.
Additional tests cover missing/reversed/duplicate/stale links, extreme raw IDs,
world replacement, repair after error, and read-only validation.

The benchmark uses chain/wide/balanced graphs of 32, 512 and 2,048 nodes. It
compares the original function, the compatibility function with fresh scratch,
and a reused validator on the same World. Construction and output are excluded;
validation, scratch rebuilding, allocation/deallocation in the fresh paths and
error checking are included. There are 3 warmups and 11 measurements per variant;
execution order rotates and `std::hint::black_box` protects inputs/results on a
best-effort basis. Every raw sample, median and nearest-rank p95 is printed.
With only 11 samples, p95 is the maximum observation, not a reliable tail-latency
estimate. Preserve slowdowns as well as improvements, especially at small sizes.

The Scene Performance workflow records the checked-out SHA (a PR merge candidate
when applicable), rustc/cargo, OS, CPU brand, logical CPU count, memory and RUSTFLAGS,
then uploads raw CSV and environment artifacts. CI runners are shared/noisy;
these are CPU microbenchmarks, not GPU measurements or end-to-end game FPS.
No wall-clock threshold is a CI gate. Correctness, operation-count invariants,
formatting, Clippy and existing workspace/MSRV/platform gates remain mandatory.
No speedup is asserted before actual execution evidence is recorded in the PR.

### Reuse review

SciRust's `scirust-graph/src/dag.rs` at
`e1237827f73268bf43b2b971674a697cd186d95e` already owns a multi-parent causal DAG.
Its dense node-index representation and constructor-validated topology are not a
drop-in validator for externally mutated ECS Parent/Children components with stale
generations. Keep that graph authority in SciRust rather than copying its data
model or adding a heavy dependency to this specialized validation seam. A shared
sparse functional-graph primitive needs an actual second consumer and compatible
tests before extraction. This increment changes no other repository.

## EE-PERF-02: transform propagation

Baseline: `propagate_transforms` at
`a00b6e513162d9a82473dfa5fe041e6cda465756`. The body is frozen in
`crates/extrem_scene/tests/support/legacy_propagation.rs` (test/example only).
Each visited node previously cloned its Children vector. The new traversal borrows
that list after writing the parent's global transform, and owns reusable pending
and visited collections. The compatibility function keeps its signature and uses
fresh scratch. The engine's existing PostUpdate closure now captures one
`TransformPropagator` per engine: this is wired into actual runtime execution,
not just exposed as an unused library API.

Root discovery, LIFO traversal order, Entity generation checks and TRS composition
are unchanged. This is full recomputation, not dirty-subtree caching. It is also
not a hierarchy validator, a numeric sanitization pass or a matrix/shear redesign.
Unreachable nodes keep their prior globals, missing child transforms are skipped,
and reachable malformed cycles terminate through the visited set, as before.
For conflicting multi-parent data, root/first-visit order remains unspecified.
Validate scenes at ingestion/edit boundaries before running the hot path.

`TransformPropagationStats` reports visited nodes, links, skipped revisits/missing
locals, first-time global insertion, defensive write errors and scratch capacities.
The engine updates the existing stats resource in place on normal frames; a
replaced App world receives a new stats resource. Scratch is cleared semantically
every call even across world replacement. No raw entity index controls allocation.
The standalone `release_memory()` drops scratch; the captured engine workspace
currently retains its high-water capacity for the lifetime of that system. Capacity
counts are not allocator counts, RSS, GPU memory or a claim of zero allocations.

Ten propagation tests cover bitwise TRS parity against the old code on the same
World, all 625 four-node parent graphs, an independent parent-first oracle on
valid shapes, deep/wide 20,000-node scenes, corruption, stale/extreme IDs, missing
components, edits/reparent/detach, signed zero, world replacement and scratch reset.
Two engine tests exercise Update -> PostUpdate -> Render, runtime scratch reuse,
resource recreation and same-valued world-local IDs after replacing the World.

```bash
cargo test -p extrem_scene --test propagation_contract --locked
cargo test -p extrem_engine --test propagation_runtime --locked
EXTREM_BENCH_REVISION="$(git rev-parse HEAD)" \
  cargo run --release -p extrem_scene --example propagation_bench --locked
```

The paired release benchmark uses chain/wide/balanced/independent-root scenes of
32, 512, 2,048 and 20,000 nodes: 3 warmups and 21 observations per variant. Each
sample changes a root input, computes an old-code reference, resets global outputs,
and rotates legacy/fresh/reused order on the same World. Every TRS field is checked
bitwise after every timed run. Setup, resets, oracle execution, comparison and
printing are outside timing. Globals are preallocated: this measures steady-state
propagation including scratch management, not scene construction or first insertion.
The raw samples, median, nearest-rank p95 (20th ordered observation of 21) and
scratch capacities are uploaded with the execution environment. This small p95 is
not a reliable tail-latency qualification. Report all regressions as well as gains.
Both old and new traversals are already linear in reachable nodes/edges under
normal hashing; this slice targets copies/allocations, not asymptotic improvement.
No FPS, GPU, end-to-end game or universal speedup claim follows from this benchmark.

## Ordered next slices (planned, not implemented here)

1. Profile render extraction and its temporary entity set/command allocations;
   retain exact camera selection and command-order comparison before optimization.
   Separately repair the externally mutable render-graph panic contract.
2. Measure ECS query and entity-count costs under churn; improve storage only when
   workloads demonstrate a benefit and stale-generation behavior remains intact.
3. Add conservative visibility/culling, draw batching and instancing to the actual
   GPU path, with CPU oracle and separately recorded GPU timing/visual evidence.
4. Develop bounded asset streaming/import, animation/skin workloads and frame-time
   instrumentation. Add LOD/DRS/ElasticXxx control only with quality budgets,
   hysteresis, cooldown and rollback, not to conceal correctness defects.

After each green merge refresh the actual default branch, inspect concurrent PRs,
record a resumable checkpoint and select the next measured bottleneck. Never
weaken security checks, fabricate measurements, or equate operation counts with FPS.
