use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Three-dimensional vector used by engine transforms and simulation code.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length_squared(self) -> f32 {
        self.x
            .mul_add(self.x, self.y.mul_add(self.y, self.z * self.z))
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length <= f32::EPSILON {
            Self::ZERO
        } else {
            self / length
        }
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x.mul_add(rhs.x, self.y.mul_add(rhs.y, self.z * rhs.z))
    }

    pub fn component_mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y.mul_add(rhs.z, -self.z * rhs.y),
            self.z.mul_add(rhs.x, -self.x * rhs.z),
            self.x.mul_add(rhs.y, -self.y * rhs.x),
        )
    }

    pub fn lerp(self, target: Self, amount: f32) -> Self {
        self + (target - self) * amount.clamp(0.0, 1.0)
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

/// Position, Euler rotation and scale of an entity in local space.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Vec3::ZERO,
        scale: Vec3::ONE,
    };

    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub const fn from_scale(scale: Vec3) -> Self {
        Self {
            scale,
            ..Self::IDENTITY
        }
    }

    pub const fn with_rotation(mut self, rotation: Vec3) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn combine(parent: Self, local: Self) -> Self {
        Self {
            translation: parent.translation + local.translation.component_mul(parent.scale),
            rotation: parent.rotation + local.rotation,
            scale: parent.scale.component_mul(local.scale),
        }
    }
}

/// Compact column-major 4x4 matrix for camera and render extraction.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Mat4 {
    pub data: [f32; 16],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        data: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn translation(offset: Vec3) -> Self {
        let mut matrix = Self::IDENTITY;
        matrix.data[12] = offset.x;
        matrix.data[13] = offset.y;
        matrix.data[14] = offset.z;
        matrix
    }

    pub fn scale(scale: Vec3) -> Self {
        let mut matrix = Self::IDENTITY;
        matrix.data[0] = scale.x;
        matrix.data[5] = scale.y;
        matrix.data[10] = scale.z;
        matrix
    }

    pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_radians * 0.5).tan();
        let inverse_depth = 1.0 / (near - far);
        Self {
            data: [
                f / aspect,
                0.0,
                0.0,
                0.0,
                0.0,
                f,
                0.0,
                0.0,
                0.0,
                0.0,
                (far + near) * inverse_depth,
                -1.0,
                0.0,
                0.0,
                (2.0 * far * near) * inverse_depth,
                0.0,
            ],
        }
    }

    pub fn orthographic(width: f32, height: f32, near: f32, far: f32) -> Self {
        Self {
            data: [
                2.0 / width,
                0.0,
                0.0,
                0.0,
                0.0,
                2.0 / height,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0 / (near - far),
                0.0,
                0.0,
                0.0,
                near / (near - far),
                1.0,
            ],
        }
    }

    pub fn multiply(self, rhs: Self) -> Self {
        let mut output = [0.0; 16];
        for column in 0..4 {
            for row in 0..4 {
                output[column * 4 + row] = (0..4)
                    .map(|index| self.data[index * 4 + row] * rhs.data[column * 4 + index])
                    .sum();
            }
        }
        Self { data: output }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Vec3};

    #[test]
    fn vector_operations_are_predictable() {
        let vector = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(vector.length(), 5.0);
        assert_eq!(vector.normalized(), Vec3::new(0.6, 0.8, 0.0));
        assert_eq!(
            Vec3::new(1.0, 0.0, 0.0).cross(Vec3::new(0.0, 1.0, 0.0)),
            Vec3::new(0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn matrix_multiplication_preserves_identity() {
        let matrix = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(matrix.multiply(Mat4::IDENTITY), matrix);
    }
}
