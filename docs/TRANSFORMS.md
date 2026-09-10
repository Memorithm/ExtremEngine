# 3D Transform Mathematics in ExtremEngine

## Local and World Transforms

Entities in ExtremEngine hold a `Transform` representing local position, orientation, and scale:

```rust
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
```

## Hierarchy Composition

When computing global transforms down a scene hierarchy, local-to-parent composition follows rigid/affine composition rules:

$$T_{\text{global}} = T_{\text{parent}} + R_{\text{parent}} \cdot (S_{\text{parent}} \odot T_{\text{local}})$$
$$R_{\text{global}} = R_{\text{parent}} \otimes R_{\text{local}}$$
$$S_{\text{global}} = S_{\text{parent}} \odot S_{\text{local}}$$

In Rust code:

```rust
pub fn combine(parent: Self, local: Self) -> Self {
    let scaled_translation = local.translation.component_mul(parent.scale);
    let rotated_translation = parent.rotation.rotate_vec3(scaled_translation);

    Self {
        translation: parent.translation + rotated_translation,
        rotation: (parent.rotation * local.rotation).normalized(),
        scale: parent.scale.component_mul(local.scale),
    }
}
```

## Camera Transformations

Camera view matrices invert the camera entity's world transform:

$$V = R_{\text{camera}}^{-1} \cdot T_{\text{camera}}^{-1}$$

Depth projection conventions match WGPU clip space expectations ([0, 1] Z depth range).
