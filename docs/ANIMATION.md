# Skeletal Animation Subsystem (`extrem_animation`)

ExtremEngine features a skeletal animation pipeline supporting clip sampling, shortest-path quaternion interpolation, pose blending, and GPU skinning palette generation.

## Key Concepts

- **Skeleton**: Hierarchical structure of `Joint`s with bind poses and inverse bind matrices.
- **Track<T>**: Keyframed data for joint translation (`Vec3`), rotation (`Quat`), or scale (`Vec3`).
- **AnimationClip**: Collection of tracks evaluated over a duration.
- **LocalPose**: Joint local transforms relative to joint parents.
- **ModelPose**: Character-space joint transforms after parent hierarchy propagation.
- **Matrix Palette**: Joint matrices ($M_{\text{model}} \cdot M_{\text{inverse\_bind}}$) formatted for GPU Linear Blend Skinning (LBS) uniform buffers.

## Quaternion Slerp & Hemisphere Alignment

To prevent 360-degree flip artifacts during quaternion interpolation, tracks and pose blending perform shortest-path slerp:

```rust
if q1.dot(q2) < 0.0 {
    q2 = -q2;
}
```

## Adaptive Execution Hooks

The `extrem_animation::adaptive` module provides trait definitions (`AdaptiveDomain`, `ProgressiveDecision`) for bounded frame-budget evaluation (EEFP / VPAE) without claiming scientific novelty.
