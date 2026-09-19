# ElasticXxx adaptive fixed-step budget integration

Status: BE15e candidate integration. This document makes no performance, FPS, GPU, energy, visual-quality, or scientific claim.

ExtremEngine exposes an optional `elastic-quality` feature that consumes only the public `elastic` facade pinned to an exact ElasticXxx revision. The first physical consumer is intentionally narrow: the existing `max_fixed_steps_per_frame` scheduler cap. This cap already controls how much catch-up simulation work may execute before overdue whole fixed steps are dropped.

The embedding supplies an explicit wall-clock frame interval through `FrameTimeObservation`; the controller does not relabel `FrameStats.submitted_commands`, `drawn_pixels`, or synthetic benchmark counters as time. Missing, non-finite, or non-positive measurements are `Unknown` and cannot authorize mutation.

A numeric hysteresis policy proposes one declared neighbouring budget value. ElasticXxx Boolean admission then requires both measurement availability and cooldown readiness. `False` and `Unknown` stop before mutation. A `True` guard still does not authorize the change: `FixedStepBudgetActuator::validate_fixed_step_budget` is called immediately before application, the resulting state is verified, and failed verification rolls back to the prior budget. If rollback itself cannot be verified, the controller latches a terminal fault and refuses every later admission until `recover_after_fault` verifies an explicit expected state. Cooldown state is recorded only after successful verification.

The built-in `Engine<R>` actuator keeps `EngineConfig.max_fixed_steps_per_frame` and the application scheduler synchronized. The trait is public so fault-injection tests can demonstrate verification failure and rollback without pretending that the in-memory setter itself is a fallible hardware actuator.

## Compatibility and licensing

The default ExtremEngine feature set and Rust 1.87 MSRV remain unchanged. `elastic-quality` is opt-in because the pinned ElasticXxx revision declares Rust 1.89. Consumers enabling it must also comply with ElasticXxx's PolyForm Noncommercial licensing terms (or a separate applicable commercial license). This integration does not relicense historical ExtremEngine code; repository-wide license provenance remains tracked separately in issue #33.

## Qualification boundary

The tests establish only deterministic controller semantics, fail-closed missing evidence, hysteresis/cooldown behavior, facade-level Boolean admission, and rollback on an injected verification failure, terminal latching after an unverified rollback, and explicit verified recovery. A real frame-time or user-experience benefit requires a separate reproducible benchmark on a declared workload and hardware. Dynamic resolution, LOD, GPU timing and visual-quality objectives remain separate future integrations.
