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
        if !length.is_finite() || length <= f32::EPSILON {
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
        let t = if amount.is_finite() {
            amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self + (target - self) * t
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

/// Quaternion representing 3D orientation. Engine-created rotations are normalized.
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
        if !length.is_finite() || length <= f32::EPSILON {
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

    pub fn is_normalized(self, tolerance: f32) -> bool {
        self.is_finite()
            && tolerance.is_finite()
            && tolerance >= 0.0
            && (self.length_squared() - 1.0).abs() <= tolerance
    }

    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    pub fn inverse(self) -> Self {
        let len_sq = self.length_squared();
        if !len_sq.is_finite() || len_sq <= f32::EPSILON {
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
        if !axis.is_finite() || !radians.is_finite() || axis.length_squared() <= f32::EPSILON {
            return Self::IDENTITY;
        }
        let norm_axis = axis.normalized();
        let half = radians * 0.5;
        let (s, c) = half.sin_cos();
        Self::new(norm_axis.x * s, norm_axis.y * s, norm_axis.z * s, c).normalized()
    }

    /// Creates a quaternion from pitch(X), yaw(Y), roll(Z), applied Z then X then Y.
    pub fn from_euler(pitch_x: f32, yaw_y: f32, roll_z: f32) -> Self {
        if !pitch_x.is_finite() || !yaw_y.is_finite() || !roll_z.is_finite() {
            return Self::IDENTITY;
        }
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

    /// Rotates a vector. The quaternion is normalized first so malformed external data cannot scale it.
    pub fn rotate_vec3(self, vec: Vec3) -> Vec3 {
        let q = self.normalized();
        let q_vec = Vec3::new(q.x, q.y, q.z);
        let uv = q_vec.cross(vec);
        let uuv = q_vec.cross(uv);
        vec + (uv * q.w + uuv) * 2.0
    }

    /// Shortest-path spherical interpolation. Non-finite interpolation factors resolve to the source.
    pub fn slerp(self, mut target: Self, t: f32) -> Self {
        let source = self.normalized();
        target = target.normalized();
        let mut cos_theta = source.dot(target).clamp(-1.0, 1.0);

        if cos_theta < 0.0 {
            target = Self::new(-target.x, -target.y, -target.z, -target.w);
            cos_theta = -cos_theta;
        }

        let clamped_t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        if cos_theta > 0.9995 {
            return Self::new(
                source.x + (target.x - source.x) * clamped_t,
                source.y + (target.y - source.y) * clamped_t,
                source.z + (target.z - source.z) * clamped_t,
                source.w + (target.w - source.w) * clamped_t,
            )
            .normalized();
        }

        let theta = cos_theta.acos();
        let sin_theta = theta.sin();
        if sin_theta.abs() <= f32::EPSILON {
            return source;
        }
        let w1 = ((1.0 - clamped_t) * theta).sin() / sin_theta;
        let w2 = (clamped_t * theta).sin() / sin_theta;
        Self::new(
            source.x * w1 + target.x * w2,
            source.y * w1 + target.y * w2,
            source.z * w1 + target.z * w2,
            source.w * w1 + target.w * w2,
        )
        .normalized()
    }

    /// Normalized linear interpolation with hemisphere alignment.
    pub fn nlerp(self, mut target: Self, t: f32) -> Self {
        let source = self.normalized();
        target = target.normalized();
        if source.dot(target) < 0.0 {
            target = Self::new(-target.x, -target.y, -target.z, -target.w);
        }
        let alpha = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        Self::new(
            source.x + (target.x - source.x) * alpha,
            source.y + (target.y - source.y) * alpha,
            source.z + (target.z - source.z) * alpha,
            source.w + (target.w - source.w) * alpha,
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
                1.0 - (yy + zz), xy + wz, xz - wy, 0.0,
                xy - wz, 1.0 - (xx + zz), yz + wx, 0.0,
                xz + wy, yz - wx, 1.0 - (xx + yy), 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }
}

impl Mul for Quat {
    type Output = Self;

    /// Hamilton product. The result is normalized because this operator is reserved for rotations.
    fn mul(self, rhs: Self) -> Self::Output {
        let a = self.normalized();
        let b = rhs.normalized();
        Self::new(
            a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
            a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        )
        .normalized()
    }
}

/// Position, quaternion rotation and scale of an entity in local space.
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
        Self { translation, ..Self::IDENTITY }
    }

    pub const fn from_scale(scale: Vec3) -> Self {
        Self { scale, ..Self::IDENTITY }
    }

    pub const fn from_rotation(rotation: Quat) -> Self {
        Self { rotation, ..Self::IDENTITY }
    }

    pub const fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn is_finite(&self) -> bool {
        self.translation.is_finite() && self.rotation.is_finite() && self.scale.is_finite()
    }

    pub fn is_valid(&self) -> bool {
        self.is_finite() && self.rotation.length_squared() > f32::EPSILON
    }

    /// Combines parent and local TRS transforms. This representation cannot exactly encode shear.
    pub fn combine(parent: Self, local: Self) -> Self {
        let scaled_translation = local.translation.component_mul(parent.scale);
        let rotated_translation = parent.rotation.rotate_vec3(scaled_translation);
        Self {
            translation: parent.translation + rotated_translation,
            rotation: parent.rotation * local.rotation,
            scale: parent.scale.component_mul(local.scale),
        }
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.translation + self.rotation.rotate_vec3(point.component_mul(self.scale))
    }

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
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
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

    /// Right-handed perspective projection matching the repository's existing convention.
    pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_radians * 0.5).tan();
        let inverse_depth = 1.0 / (near - far);
        Self {
            data: [
                f / aspect, 0.0, 0.0, 0.0,
                0.0, f, 0.0, 0.0,
                0.0, 0.0, (far + near) * inverse_depth, -1.0,
                0.0, 0.0, (2.0 * far * near) * inverse_depth, 0.0,
            ],
        }
    }

    pub fn orthographic(width: f32, height: f32, near: f32, far: f32) -> Self {
        Self {
            data: [
                2.0 / width, 0.0, 0.0, 0.0,
                0.0, 2.0 / height, 0.0, 0.0,
                0.0, 0.0, 1.0 / (near - far), 0.0,
                0.0, 0.0, near / (near - far), 1.0,
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

    pub fn transform_point3(self, p: Vec3) -> Vec3 {
        let x = self.data[0] * p.x + self.data[4] * p.y + self.data[8] * p.z + self.data[12];
        let y = self.data[1] * p.x + self.data[5] * p.y + self.data[9] * p.z + self.data[13];
        let z = self.data[2] * p.x + self.data[6] * p.y + self.data[10] * p.z + self.data[14];
        let w = self.data[3] * p.x + self.data[7] * p.y + self.data[11] * p.z + self.data[15];
        if w.is_finite() && w.abs() > f32::EPSILON && (w - 1.0).abs() > f32::EPSILON {
            Vec3::new(x / w, y / w, z / w)
        } else {
            Vec3::new(x, y, z)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Quat, Transform, Vec3};

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }

    fn approx_vec(a: Vec3, b: Vec3) {
        approx(a.x, b.x);
        approx(a.y, b.y);
        approx(a.z, b.z);
    }

    #[test]
    fn vector_operations_are_predictable() {
        let vector = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(vector.length(), 5.0);
        assert_eq!(vector.normalized(), Vec3::new(0.6, 0.8, 0.0));
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
    }

    #[test]
    fn matrix_multiplication_preserves_identity() {
        let matrix = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(matrix.multiply(Mat4::IDENTITY), matrix);
    }

    #[test]
    fn hamilton_product_matches_sequential_rotations() {
        let qx = Quat::from_axis_angle(Vec3::X, 0.61);
        let qy = Quat::from_axis_angle(Vec3::Y, -0.37);
        let qz = Quat::from_axis_angle(Vec3::Z, 1.13);
        let point = Vec3::new(0.3, -2.0, 4.5);

        approx_vec((qy * qx).rotate_vec3(point), qy.rotate_vec3(qx.rotate_vec3(point)));
        approx_vec(
            (qz * qy * qx).rotate_vec3(point),
            qz.rotate_vec3(qy.rotate_vec3(qx.rotate_vec3(point))),
        );
    }

    #[test]
    fn quaternion_inverse_round_trips_rotation() {
        let q = Quat::from_euler(0.4, -0.8, 0.3);
        let point = Vec3::new(3.0, -1.0, 2.0);
        approx_vec(q.inverse().rotate_vec3(q.rotate_vec3(point)), point);
        assert!(q.is_normalized(1e-5));
    }

    #[test]
    fn parent_and_local_nontrivial_rotations_compose_correctly() {
        let parent = Transform {
            translation: Vec3::new(10.0, 2.0, -3.0),
            rotation: Quat::from_axis_angle(Vec3::Y, 0.9),
            scale: Vec3::new(2.0, 2.0, 2.0),
        };
        let local = Transform {
            translation: Vec3::new(1.0, -0.5, 2.0),
            rotation: Quat::from_axis_angle(Vec3::X, -0.6),
            scale: Vec3::new(0.5, 0.5, 0.5),
        };
        let point = Vec3::new(0.4, 0.2, -0.8);
        let combined = Transform::combine(parent, local);
        approx_vec(combined.transform_point(point), parent.transform_point(local.transform_point(point)));
    }

    #[test]
    fn quaternion_slerp_shortest_path() {
        let q1 = Quat::IDENTITY;
        let q2 = Quat::from_axis_angle(Vec3::Z, std::f32::consts::PI - 0.1);
        let slerped = q1.slerp(q2, 0.5);
        assert!(slerped.is_finite());
        approx(slerped.length(), 1.0);
    }

    #[test]
    fn transform_to_mat4_matches_direct_transform() {
        let transform = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_4),
            scale: Vec3::new(2.0, 2.0, 2.0),
        };
        let point = Vec3::new(0.5, -1.0, 2.0);
        approx_vec(transform.to_mat4().transform_point3(point), transform.transform_point(point));
    }

    #[test]
    fn non_finite_interpolation_factor_does_not_poison_rotation() {
        let q = Quat::from_axis_angle(Vec3::Y, 1.0);
        assert_eq!(Quat::IDENTITY.slerp(q, f32::NAN), Quat::IDENTITY);
    }
}