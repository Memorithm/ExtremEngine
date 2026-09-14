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

## Ordered next slices (planned, not implemented here)

1. Profile transform propagation and render extraction; remove repeated child-list
   clones and reusable-scratch allocations with exact output comparison.
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
