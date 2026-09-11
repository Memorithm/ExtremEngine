# 3D Transform Mathematics in ExtremEngine

## Representation

`Transform` stores translation, quaternion orientation and component-wise scale:

```rust
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
```

Engine-created rotations use normalized quaternions. `Quat` multiplication implements the Hamilton product and is regression-tested against sequential non-trivial rotations.

## Hierarchy composition

For representable TRS composition:

$$T_g = T_p + R_p(S_p \odot T_l)$$

$$R_g = R_p \otimes R_l$$

$$S_g = S_p \odot S_l$$

Parent rotation therefore rotates the scaled child translation; rotations are not added as Euler vectors.

### Important TRS limitation

A single translation/rotation/component-scale triple cannot represent arbitrary shear. Combining non-uniform scale with differently oriented parent/child transforms can mathematically generate shear. `Transform::combine` remains a TRS operation and must not be described as an exact representation of every affine transform. Systems that require arbitrary affine composition should operate on `Mat4` or a future explicit affine type.

## Camera transformations

Camera view matrices invert world rotation and translation:

$$V = R^{-1} T^{-1}$$

Camera scale is not part of the viewing model.

## WebGPU projection convention

ExtremEngine uses a right-handed camera convention looking along negative Z and maps depth into WebGPU's normalized range:

- near plane → `z = 0`
- far plane → `z = 1`

Both perspective and orthographic matrices use this convention. Regression tests project explicit near/far points to guard against accidental reintroduction of OpenGL `[-1,1]` depth formulas.
