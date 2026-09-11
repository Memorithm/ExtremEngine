# Skeletal Animation Subsystem (`extrem_animation`)

`extrem_animation` is the current low-level skeletal-animation foundation. It is not yet a complete character-animation stack or GPU skinning pipeline.

## Implemented runtime contracts

- `Skeleton` / `Joint`: topologically ordered hierarchy with finite bind transforms and inverse-bind matrices.
- `Track<T>` / `Keyframe<T>`: strict time ordering; duplicate times are rejected.
- `AnimationClip`: finite non-negative duration and finite values; rotation keys must contain normalized quaternions.
- `LocalPose` / `ModelPose`: exact pose-length validation against the skeleton; mismatched poses fail instead of being truncated.
- sampling: invalid joint targets and non-finite sample times return `AnimationError` rather than being ignored.
- quaternion interpolation: `Linear` uses shortest-hemisphere normalized linear interpolation (`nlerp`); `Slerp` uses shortest-path spherical interpolation.
- pose blending: translation/scale lerp and quaternion slerp after shape validation.
- matrix palette: `model_transform * inverse_bind_matrix`, returned only when pose and skeleton sizes agree.
- `AnimationPlayer`: runtime delta/time/playback rate are checked for finite values.

The current matrix palette is a CPU data product ready to feed a future LBS shader. This does **not** mean GPU skinning is implemented yet.

## Adaptive execution research contracts

The `adaptive` module contains experimental extension contracts for EEFP/VPAE. These names are working research labels and are not novelty claims.

`AdaptiveDomain` now models a transaction boundary:

```text
CHECKPOINT
→ OBSERVE
→ PROPOSE
→ VALIDATE
→ APPLY
→ VERIFY
→ COMMIT
          └─ failure → ROLLBACK
```

`ProgressiveEvaluator` provides a separate VPAE-style decision surface:

```text
hard invariants false → Recover
hard invariants true  → observe → Continue | Accept | Recover
```

No learned policy is bundled. Hard correctness invariants are deliberately separate from frame-budget objectives.

## Deferred work

- glTF/GLB skin/animation import;
- animation events/notifies;
- additive/layer masks and N-way blends;
- root motion;
- animation graph/state machine;
- IK/retargeting;
- morph targets;
- actual GPU LBS/DQS shaders;
- motion matching and Recovery-Aware Motion Matching experiments.
