use extrem_ecs::World;
use extrem_math::{Mat4, Quat, Transform, Vec3};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier for a joint within a skeleton hierarchy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct JointId(pub usize);

/// Joint node representation with bind pose and inverse bind matrix.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Joint {
    pub name: String,
    pub parent: Option<JointId>,
    pub bind_pose: Transform,
    pub inverse_bind_matrix: Mat4,
}

/// Errors produced during skeleton validation, animation clip creation, or sampling.
#[derive(Debug, PartialEq, Eq)]
pub enum AnimationError {
    InvalidTransform,
    InvalidParentIndex(usize),
    NonTopologicalParent,
    NegativeDuration,
    UnsortedKeyframes,
    NonFiniteTime,
}

impl fmt::Display for AnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransform => {
                write!(formatter, "transform contains NaN or infinite values")
            }
            Self::InvalidParentIndex(idx) => {
                write!(formatter, "joint parent index {idx} out of bounds")
            }
            Self::NonTopologicalParent => {
                write!(formatter, "parent index must be smaller than joint index")
            }
            Self::NegativeDuration => write!(formatter, "clip duration must be non-negative"),
            Self::UnsortedKeyframes => {
                write!(formatter, "keyframes must be strictly sorted by time")
            }
            Self::NonFiniteTime => write!(formatter, "sampling time must be finite and non-NaN"),
        }
    }
}

impl std::error::Error for AnimationError {}

/// Verified skeleton hierarchy for skeletal mesh skinning and pose propagation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skeleton {
    joints: Vec<Joint>,
}

impl Skeleton {
    pub fn new(joints: Vec<Joint>) -> Result<Self, AnimationError> {
        let skel = Self { joints };
        skel.validate()?;
        Ok(skel)
    }

    pub fn validate(&self) -> Result<(), AnimationError> {
        for (idx, joint) in self.joints.iter().enumerate() {
            if !joint.bind_pose.is_finite() {
                return Err(AnimationError::InvalidTransform);
            }
            if let Some(parent) = joint.parent {
                if parent.0 >= self.joints.len() {
                    return Err(AnimationError::InvalidParentIndex(parent.0));
                }
                if parent.0 >= idx {
                    return Err(AnimationError::NonTopologicalParent);
                }
            }
        }
        Ok(())
    }

    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    pub fn len(&self) -> usize {
        self.joints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }
}

/// Keyframe interpolation mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InterpolationMode {
    Step,
    Linear,
    Slerp,
}

/// Single timed keyframe value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}

/// Animation track bound to a joint ID.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track<T> {
    pub joint: JointId,
    pub interpolation: InterpolationMode,
    pub keyframes: Vec<Keyframe<T>>,
}

impl<T> Track<T> {
    pub fn validate(&self) -> Result<(), AnimationError> {
        let mut last_time = -f32::EPSILON;
        for kf in &self.keyframes {
            if !kf.time.is_finite() || kf.time < 0.0 {
                return Err(AnimationError::NonFiniteTime);
            }
            if kf.time < last_time {
                return Err(AnimationError::UnsortedKeyframes);
            }
            last_time = kf.time;
        }
        Ok(())
    }
}

/// Keyframed animation clip for skeletal hierarchy animation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub translation_tracks: Vec<Track<Vec3>>,
    pub rotation_tracks: Vec<Track<Quat>>,
    pub scale_tracks: Vec<Track<Vec3>>,
}

impl AnimationClip {
    pub fn new(
        name: impl Into<String>,
        duration: f32,
        translation_tracks: Vec<Track<Vec3>>,
        rotation_tracks: Vec<Track<Quat>>,
        scale_tracks: Vec<Track<Vec3>>,
    ) -> Result<Self, AnimationError> {
        if !duration.is_finite() || duration < 0.0 {
            return Err(AnimationError::NegativeDuration);
        }
        for track in &translation_tracks {
            track.validate()?;
        }
        for track in &rotation_tracks {
            track.validate()?;
        }
        for track in &scale_tracks {
            track.validate()?;
        }
        Ok(Self {
            name: name.into(),
            duration,
            translation_tracks,
            rotation_tracks,
            scale_tracks,
        })
    }
}

/// Local joint transforms in skeleton parent space.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalPose {
    pub transforms: Vec<Transform>,
}

impl LocalPose {
    pub fn from_bind_pose(skeleton: &Skeleton) -> Self {
        Self {
            transforms: skeleton.joints().iter().map(|j| j.bind_pose).collect(),
        }
    }
}

/// Model-space (character space) transforms after parent transform propagation.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPose {
    pub transforms: Vec<Transform>,
}

impl ModelPose {
    pub fn from_local_pose(local_pose: &LocalPose, skeleton: &Skeleton) -> Self {
        let mut model_transforms = Vec::with_capacity(skeleton.len());
        for (idx, joint) in skeleton.joints().iter().enumerate() {
            let local = local_pose
                .transforms
                .get(idx)
                .copied()
                .unwrap_or(joint.bind_pose);
            let model = match joint.parent {
                Some(parent) => Transform::combine(model_transforms[parent.0], local),
                None => local,
            };
            model_transforms.push(model);
        }
        Self {
            transforms: model_transforms,
        }
    }

    /// Computes joint matrix palette (model_transform * inverse_bind_matrix) for GPU skinning shaders.
    pub fn matrix_palette(&self, skeleton: &Skeleton) -> Vec<Mat4> {
        self.transforms
            .iter()
            .zip(skeleton.joints())
            .map(|(model_transform, joint)| {
                model_transform
                    .to_mat4()
                    .multiply(joint.inverse_bind_matrix)
            })
            .collect()
    }
}

/// Evaluates sample time based on clip duration and looping settings.
pub fn clamp_or_loop_time(time: f32, duration: f32, looping: bool) -> f32 {
    if duration <= f32::EPSILON {
        return 0.0;
    }
    if looping {
        let rem = time % duration;
        if rem < 0.0 { rem + duration } else { rem }
    } else {
        time.clamp(0.0, duration)
    }
}

/// Samples a Vec3 track at a given time.
fn sample_vec3_track(track: &Track<Vec3>, time: f32, default: Vec3) -> Vec3 {
    if track.keyframes.is_empty() {
        return default;
    }
    if track.keyframes.len() == 1 || time <= track.keyframes[0].time {
        return track.keyframes[0].value;
    }
    let last = track.keyframes.last().unwrap();
    if time >= last.time {
        return last.value;
    }

    for window in track.keyframes.windows(2) {
        if time >= window[0].time && time <= window[1].time {
            let dt = window[1].time - window[0].time;
            let factor = if dt <= f32::EPSILON {
                0.0
            } else {
                (time - window[0].time) / dt
            };
            return match track.interpolation {
                InterpolationMode::Step => window[0].value,
                InterpolationMode::Linear | InterpolationMode::Slerp => {
                    window[0].value.lerp(window[1].value, factor)
                }
            };
        }
    }
    last.value
}

/// Samples a Quat track at a given time with shortest-path slerp.
fn sample_quat_track(track: &Track<Quat>, time: f32, default: Quat) -> Quat {
    if track.keyframes.is_empty() {
        return default;
    }
    if track.keyframes.len() == 1 || time <= track.keyframes[0].time {
        return track.keyframes[0].value;
    }
    let last = track.keyframes.last().unwrap();
    if time >= last.time {
        return last.value;
    }

    for window in track.keyframes.windows(2) {
        if time >= window[0].time && time <= window[1].time {
            let dt = window[1].time - window[0].time;
            let factor = if dt <= f32::EPSILON {
                0.0
            } else {
                (time - window[0].time) / dt
            };
            return match track.interpolation {
                InterpolationMode::Step => window[0].value,
                InterpolationMode::Linear | InterpolationMode::Slerp => {
                    window[0].value.slerp(window[1].value, factor)
                }
            };
        }
    }
    last.value
}

/// Samples an animation clip onto a skeleton, producing a local joint pose.
pub fn sample_clip(
    clip: &AnimationClip,
    skeleton: &Skeleton,
    time: f32,
    looping: bool,
) -> LocalPose {
    let mut pose = LocalPose::from_bind_pose(skeleton);
    let sample_time = clamp_or_loop_time(time, clip.duration, looping);

    for track in &clip.translation_tracks {
        let idx = track.joint.0;
        if idx < pose.transforms.len() {
            let default_val = pose.transforms[idx].translation;
            pose.transforms[idx].translation = sample_vec3_track(track, sample_time, default_val);
        }
    }

    for track in &clip.rotation_tracks {
        let idx = track.joint.0;
        if idx < pose.transforms.len() {
            let default_val = pose.transforms[idx].rotation;
            pose.transforms[idx].rotation = sample_quat_track(track, sample_time, default_val);
        }
    }

    for track in &clip.scale_tracks {
        let idx = track.joint.0;
        if idx < pose.transforms.len() {
            let default_val = pose.transforms[idx].scale;
            pose.transforms[idx].scale = sample_vec3_track(track, sample_time, default_val);
        }
    }

    pose
}

/// Blends two local poses with normalized blend weight `amount` in [0, 1].
pub fn blend_poses(pose_a: &LocalPose, pose_b: &LocalPose, amount: f32) -> LocalPose {
    let alpha = amount.clamp(0.0, 1.0);
    let len = pose_a.transforms.len().min(pose_b.transforms.len());
    let mut blended = Vec::with_capacity(len);

    for idx in 0..len {
        let t_a = pose_a.transforms[idx];
        let t_b = pose_b.transforms[idx];

        blended.push(Transform {
            translation: t_a.translation.lerp(t_b.translation, alpha),
            rotation: t_a.rotation.slerp(t_b.rotation, alpha),
            scale: t_a.scale.lerp(t_b.scale, alpha),
        });
    }

    LocalPose {
        transforms: blended,
    }
}

/// ECS Component that holds active animation playback state.
#[derive(Clone, Debug)]
pub struct AnimationPlayer {
    pub clip: Option<AnimationClip>,
    pub time: f32,
    pub speed: f32,
    pub looping: bool,
    pub playing: bool,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self {
            clip: None,
            time: 0.0,
            speed: 1.0,
            looping: true,
            playing: true,
        }
    }
}

impl AnimationPlayer {
    pub fn play(&mut self, clip: AnimationClip) {
        self.clip = Some(clip);
        self.time = 0.0;
        self.playing = true;
    }

    pub fn advance(&mut self, delta_seconds: f32) {
        if !self.playing {
            return;
        }
        if let Some(clip) = &self.clip {
            self.time += delta_seconds * self.speed;
            self.time = clamp_or_loop_time(self.time, clip.duration, self.looping);
        }
    }
}

/// System for advancing animation players and sampling pose onto entities.
pub fn update_animation_players(world: &mut World, delta_seconds: f32) {
    for (_entity, player) in world.iter_mut::<AnimationPlayer>() {
        player.advance(delta_seconds);
    }
}

/// Interfaces and data structures for research extension points (EEFP, VPAE).
pub mod adaptive {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ProgressiveDecision {
        Continue,
        Accept,
        Recover,
    }

    /// Systems contract for adaptive evaluation under hard invariant constraints.
    pub trait AdaptiveDomain {
        type State;
        type Observation;
        type Plan;
        type Error;

        fn observe(&self, state: &Self::State) -> Self::Observation;
        fn propose(&mut self, observation: &Self::Observation) -> Result<Self::Plan, Self::Error>;
        fn validate(&self, plan: &Self::Plan) -> bool;
        fn apply(&mut self, state: &mut Self::State, plan: Self::Plan) -> Result<(), Self::Error>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_creation_and_validation() {
        let root = Joint {
            name: "root".to_string(),
            parent: None,
            bind_pose: Transform::IDENTITY,
            inverse_bind_matrix: Mat4::IDENTITY,
        };
        let spine = Joint {
            name: "spine".to_string(),
            parent: Some(JointId(0)),
            bind_pose: Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -1.0, 0.0)),
        };

        let skeleton = Skeleton::new(vec![root, spine]).expect("valid skeleton");
        assert_eq!(skeleton.len(), 2);
    }

    #[test]
    fn animation_clip_sampling_and_blending() {
        let root = Joint {
            name: "root".to_string(),
            parent: None,
            bind_pose: Transform::IDENTITY,
            inverse_bind_matrix: Mat4::IDENTITY,
        };
        let skeleton = Skeleton::new(vec![root]).expect("skeleton");

        let track = Track {
            joint: JointId(0),
            interpolation: InterpolationMode::Linear,
            keyframes: vec![
                Keyframe {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                Keyframe {
                    time: 1.0,
                    value: Vec3::new(10.0, 0.0, 0.0),
                },
            ],
        };
        let clip = AnimationClip::new("walk", 1.0, vec![track], vec![], vec![]).expect("clip");

        let pose_half = sample_clip(&clip, &skeleton, 0.5, false);
        assert!((pose_half.transforms[0].translation.x - 5.0).abs() < 1e-4);

        let pose_full = sample_clip(&clip, &skeleton, 1.0, false);
        let blended = blend_poses(&pose_half, &pose_full, 0.5);
        assert!((blended.transforms[0].translation.x - 7.5).abs() < 1e-4);
    }

    #[test]
    fn model_pose_and_matrix_palette_computation() {
        let root = Joint {
            name: "root".to_string(),
            parent: None,
            bind_pose: Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -1.0, 0.0)),
        };
        let child = Joint {
            name: "child".to_string(),
            parent: Some(JointId(0)),
            bind_pose: Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -3.0, 0.0)),
        };

        let skeleton = Skeleton::new(vec![root, child]).expect("skeleton");
        let local_pose = LocalPose::from_bind_pose(&skeleton);
        let model_pose = ModelPose::from_local_pose(&local_pose, &skeleton);

        assert_eq!(model_pose.transforms[1].translation.y, 3.0);

        let palette = model_pose.matrix_palette(&skeleton);
        assert_eq!(palette.len(), 2);
    }
}
