# ExtremEngine agent contract

## Repository trunk

The GitHub default branch and authoritative product trunk for this repository is:

```text
agent/initial-engine
```

Autonomous engineering work must branch from the current head of `agent/initial-engine` and target pull requests back to `agent/initial-engine` unless a repository-level migration explicitly changes the GitHub default branch.

Do not assume that a branch named `main` is authoritative when it differs from the configured GitHub default branch.

After every merge, refresh the current default-branch head before starting the next increment.

## Engineering boundaries

- Preserve the existing crate architecture; avoid parallel rewrites of capabilities already owned by another crate.
- Keep the real GPU authority in the existing GPU layer. Browser/WASM integration should consume that boundary rather than creating a second independent renderer stack.
- Treat Rust/WASM/WebGPU, glTF/GLB, Meshopt/Draco/KTX2, LOD, dynamic resolution and frame pacing as incremental product capabilities with tests and explicit contracts.
- Do not claim performance from operation counts or architectural expectations. Hardware performance claims require reproducible measurements tied to an exact commit and environment.
- Keep unsafe code, external data, asset parsing and browser/GPU capability discovery fail-closed where the existing contracts require it.
- Reusable generic primitives that belong in the wider Memorithm ecosystem should be promoted deliberately rather than copied into multiple repositories.

## Pull-request discipline

A pull request may be merged only when it is non-draft, conflict-free/mergeable, and every applicable required gate is green on its exact head SHA. A CI result from an earlier head is not evidence for a later head.


## AX-inspired isolated task execution

Before work involving autonomous execution, generated or untrusted code, remote workers, task-scoped tool authority, workspace materialization, resource envelopes, network egress, sandboxing, checkpoint/suspend/resume, or cross-repository execution, also read:

`origin/agent/ecosystem-roadmap:.agent/EXTREMENGINE_ECOSYSTEM_ROADMAP.yaml`

Google AX is an architectural reference only. The Memorithm contract is Rust-native: SciRust Hub owns task lifecycle/identity, RemoteOps owns concrete host enforcement, ElasticXxx owns adaptive resource policy, and this repository retains its domain authority. A supervised process must never be described as a hostile-code sandbox.
