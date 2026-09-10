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
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
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

/// Unit quaternion representing 3D orientation.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    pub fn length_squared(self) -> f32 {
        self.x.mul_add(
            self.x,
            self.y
                .mul_add(self.y, self.z.mul_add(self.z, self.w * self.w)),
        )
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length <= f32::EPSILON {
            Self::IDENTITY
        } else {
            Self::new(
                self.x / length,
                self.y / length,
                self.z / length,
                self.w / length,
            )
        }
    }

    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    pub fn inverse(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq <= f32::EPSILON {
            Self::IDENTITY
        } else {
            Self::new(
                -self.x / len_sq,
                -self.y / len_sq,
                -self.z / len_sq,
                self.w / len_sq,
            )
        }
    }

    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Self {
        let norm_axis = axis.normalized();
        let half = radians * 0.5;
        let s = half.sin();
        let c = half.cos();
        Self::new(norm_axis.x * s, norm_axis.y * s, norm_axis.z * s, c).normalized()
    }

    /// Creates quaternion from Euler angles (roll = Z, pitch = X, yaw = Y) in radians (Z-X-Y order).
    pub fn from_euler(pitch_x: f32, yaw_y: f32, roll_z: f32) -> Self {
        let qx = Self::from_axis_angle(Vec3::X, pitch_x);
        let qy = Self::from_axis_angle(Vec3::Y, yaw_y);
        let qz = Self::from_axis_angle(Vec3::Z, roll_z);
        qy * qx * qz
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x.mul_add(
            rhs.x,
            self.y.mul_add(rhs.y, self.z.mul_add(rhs.z, self.w * rhs.w)),
        )
    }

    /// Rotates a 3D vector by this quaternion.
    pub fn rotate_vec3(self, vec: Vec3) -> Vec3 {
        let q_vec = Vec3::new(self.x, self.y, self.z);
        let uv = q_vec.cross(vec);
        let uuv = q_vec.cross(uv);
        vec + (uv * self.w + uuv) * 2.0
    }

    /// Spherical linear interpolation between quaternions.
    pub fn slerp(self, mut target: Self, t: f32) -> Self {
        let mut cos_theta = self.dot(target);

        // Ensure shortest path
        if cos_theta < 0.0 {
            target = Self::new(-target.x, -target.y, -target.z, -target.w);
            cos_theta = -cos_theta;
        }

        let clamped_t = t.clamp(0.0, 1.0);

        if cos_theta > 0.9995 {
            // Linear interpolation when very close to avoid division by zero
            return Self::new(
                self.x + (target.x - self.x) * clamped_t,
                self.y + (target.y - self.y) * clamped_t,
                self.z + (target.z - self.z) * clamped_t,
                self.w + (target.w - self.w) * clamped_t,
            )
            .normalized();
        }

        let theta = cos_theta.acos();
        let sin_theta = theta.sin();
        let w1 = ((1.0 - clamped_t) * theta).sin() / sin_theta;
        let w2 = (clamped_t * theta).sin() / sin_theta;

        Self::new(
            self.x * w1 + target.x * w2,
            self.y * w1 + target.y * w2,
            self.z * w1 + target.z * w2,
            self.w * w1 + target.w * w2,
        )
        .normalized()
    }

    pub fn to_mat4(self) -> Mat4 {
        let q = self.normalized();
        let x2 = q.x + q.x;
        let y2 = q.y + q.y;
        let z2 = q.z + q.z;

        let xx = q.x * x2;
        let xy = q.x * y2;
        let xz = q.x * z2;
        let yy = q.y * y2;
        let yz = q.y * z2;
        let zz = q.z * z2;
        let wx = q.w * x2;
        let wy = q.w * y2;
        let wz = q.w * z2;

        Mat4 {
            data: [
                1.0 - (yy + zz),
                xy + wz,
                xz - wy,
                0.0,
                xy - wz,
                1.0 - (xx + zz),
                yz + wx,
                0.0,
                xz + wy,
                yz - wx,
                1.0 - (xx + yy),
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        }
    }
}

impl Mul for Quat {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(
            self.w.mul_add(
                rhs.x,
                self.x
                    .mul_add(rhs.w, self.y.mul_add(rhs.z, -self.z * rhs.y)),
            ),
            self.w.mul_add(
                rhs.y,
                -self.x.mul_add(rhs.z, self.y.mul_add(rhs.w, self.z * rhs.x)),
            ),
            self.w.mul_add(
                rhs.z,
                self.x
                    .mul_add(rhs.y, -self.y.mul_add(rhs.x, self.z * rhs.w)),
            ),
            self.w.mul_add(
                rhs.w,
                -self
                    .x
                    .mul_add(rhs.x, -self.y.mul_add(rhs.y, -self.z * rhs.z)),
            ),
        )
        .normalized()
    }
}

/// Position, Quaternion rotation and scale of an entity in local space.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
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
        rotation: Quat::IDENTITY,
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

    pub const fn from_rotation(rotation: Quat) -> Self {
        Self {
            rotation,
            ..Self::IDENTITY
        }
    }

    pub const fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn is_finite(&self) -> bool {
        self.translation.is_finite() && self.rotation.is_finite() && self.scale.is_finite()
    }

    /// Combines parent and local transforms using rigid/affine hierarchy composition.
    pub fn combine(parent: Self, local: Self) -> Self {
        let scaled_translation = local.translation.component_mul(parent.scale);
        let rotated_translation = parent.rotation.rotate_vec3(scaled_translation);

        Self {
            translation: parent.translation + rotated_translation,
            rotation: (parent.rotation * local.rotation).normalized(),
            scale: parent.scale.component_mul(local.scale),
        }
    }

    /// Converts this transform to a 4x4 column-major matrix.
    pub fn to_mat4(self) -> Mat4 {
        Mat4::translation(self.translation)
            .multiply(self.rotation.to_mat4())
            .multiply(Mat4::scale(self.scale))
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

    /// Transform a 3D point (w = 1.0) by this matrix.
    pub fn transform_point3(self, p: Vec3) -> Vec3 {
        let x = self.data[0] * p.x + self.data[4] * p.y + self.data[8] * p.z + self.data[12];
        let y = self.data[1] * p.x + self.data[5] * p.y + self.data[9] * p.z + self.data[13];
        let z = self.data[2] * p.x + self.data[6] * p.y + self.data[10] * p.z + self.data[14];
        let w = self.data[3] * p.x + self.data[7] * p.y + self.data[11] * p.z + self.data[15];

        if w.abs() > f32::EPSILON && (w - 1.0).abs() > f32::EPSILON {
            Vec3::new(x / w, y / w, z / w)
        } else {
            Vec3::new(x, y, z)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Quat, Transform, Vec3};

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

    #[test]
    fn quaternion_rotation_and_composition_are_correct() {
        let q_y90 = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2);
        let vec = Vec3::new(1.0, 0.0, 0.0);
        let rotated = q_y90.rotate_vec3(vec);
        assert!((rotated.x - 0.0).abs() < 1e-5);
        assert!((rotated.z - (-1.0)).abs() < 1e-5);

        let parent = Transform {
            translation: Vec3::new(10.0, 0.0, 0.0),
            rotation: q_y90,
            scale: Vec3::ONE,
        };
        let local = Transform::from_translation(Vec3::new(0.0, 0.0, 1.0));
        let combined = Transform::combine(parent, local);

        // rotated (0,0,1) around Y by 90 deg is (1,0,0)
        assert!((combined.translation.x - 11.0).abs() < 1e-5);
        assert!((combined.translation.z - 0.0).abs() < 1e-5);
    }

    #[test]
    fn quaternion_slerp_shortest_path() {
        let q1 = Quat::IDENTITY;
        let q2 = Quat::from_axis_angle(Vec3::Z, std::f32::consts::PI - 0.1);
        let slerped = q1.slerp(q2, 0.5);
        assert!(slerped.is_finite());
        assert!((slerped.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn transform_to_mat4_matches_point_transform() {
        let q = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_4);
        let t = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: q,
            scale: Vec3::new(2.0, 2.0, 2.0),
        };
        let p = Vec3::new(0.5, -1.0, 2.0);

        let mat = t.to_mat4();
        let p_mat = mat.transform_point3(p);

        let scaled = p.component_mul(t.scale);
        let rotated = t.rotation.rotate_vec3(scaled);
        let p_direct = t.translation + rotated;

        assert!((p_mat.x - p_direct.x).abs() < 1e-4);
        assert!((p_mat.y - p_direct.y).abs() < 1e-4);
        assert!((p_mat.z - p_direct.z).abs() < 1e-4);
    }
}
