use extrem_ecs::World;
use extrem_math::{Mat4, Quat, Transform, Vec3};
use serde::{Deserialize, Serialize};
use std::fmt;

const QUATERNION_NORM_TOLERANCE: f32 = 1.0e-3;

/// Stable identifier for a joint within a skeleton hierarchy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct JointId(pub usize);

/// Joint node representation with bind pose and inverse bind matrix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    pub name: String,
    pub parent: Option<JointId>,
    pub bind_pose: Transform,
    pub inverse_bind_matrix: Mat4,
}

/// Errors produced during skeleton validation, animation clip creation, sampling, and blending.
#[derive(Debug, PartialEq, Eq)]
pub enum AnimationError {
    InvalidTransform,
    InvalidInverseBindMatrix,
    InvalidParentIndex(usize),
    NonTopologicalParent,
    InvalidDuration,
    UnsortedKeyframes,
    DuplicateKeyframeTime,
    KeyframeAfterDuration,
    NonFiniteTime,
    NonFiniteValue,
    InvalidQuaternion,
    JointOutOfRange { joint: usize, skeleton_len: usize },
    PoseLengthMismatch { left: usize, right: usize },
    PoseSkeletonMismatch { pose_len: usize, skeleton_len: usize },
    InvalidBlendWeight,
    InvalidPlaybackRate,
    InvalidDeltaTime,
}

impl fmt::Display for AnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransform => write!(formatter, "transform contains invalid numeric data"),
            Self::InvalidInverseBindMatrix => write!(formatter, "inverse bind matrix contains NaN or infinity"),
            Self::InvalidParentIndex(index) => write!(formatter, "joint parent index {index} is out of bounds"),
            Self::NonTopologicalParent => write!(formatter, "parent index must be smaller than joint index"),
            Self::InvalidDuration => write!(formatter, "clip duration must be finite and non-negative"),
            Self::UnsortedKeyframes => write!(formatter, "keyframes must be strictly sorted by time"),
            Self::DuplicateKeyframeTime => write!(formatter, "duplicate keyframe times are not allowed"),
            Self::KeyframeAfterDuration => write!(formatter, "keyframe time exceeds clip duration"),
            Self::NonFiniteTime => write!(formatter, "animation time must be finite"),
            Self::NonFiniteValue => write!(formatter, "animation value contains NaN or infinity"),
            Self::InvalidQuaternion => write!(formatter, "animation quaternion must be finite and normalized"),
            Self::JointOutOfRange { joint, skeleton_len } => {
                write!(formatter, "track joint {joint} is outside skeleton length {skeleton_len}")
            }
            Self::PoseLengthMismatch { left, right } => {
                write!(formatter, "pose lengths differ: {left} versus {right}")
            }
            Self::PoseSkeletonMismatch { pose_len, skeleton_len } => {
                write!(formatter, "pose length {pose_len} does not match skeleton length {skeleton_len}")
            }
            Self::InvalidBlendWeight => write!(formatter, "blend weight must be finite"),
            Self::InvalidPlaybackRate => write!(formatter, "animation playback rate must be finite"),
            Self::InvalidDeltaTime => write!(formatter, "animation delta time must be finite and non-negative"),
        }
    }
}

impl std::error::Error for AnimationError {}

/// Verified skeleton hierarchy for skeletal mesh skinning and pose propagation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skeleton {
    joints: Vec<Joint>,
}

impl Skeleton {
    pub fn new(joints: Vec<Joint>) -> Result<Self, AnimationError> {
        let skeleton = Self { joints };
        skeleton.validate()?;
        Ok(skeleton)
    }

    pub fn validate(&self) -> Result<(), AnimationError> {
        for (index, joint) in self.joints.iter().enumerate() {
            if !joint.bind_pose.is_valid() {
                return Err(AnimationError::InvalidTransform);
            }
            if !joint.inverse_bind_matrix.data.iter().all(|value| value.is_finite()) {
                return Err(AnimationError::InvalidInverseBindMatrix);
            }
            if let Some(parent) = joint.parent {
                if parent.0 >= self.joints.len() {
                    return Err(AnimationError::InvalidParentIndex(parent.0));
                }
                if parent.0 >= index {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InterpolationMode {
    Step,
    /// Normalized linear interpolation for quaternions; ordinary lerp for vectors.
    Linear,
    /// Spherical interpolation for quaternions; ordinary lerp for vectors.
    Slerp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track<T> {
    pub joint: JointId,
    pub interpolation: InterpolationMode,
    pub keyframes: Vec<Keyframe<T>>,
}

impl<T> Track<T> {
    pub fn validate_times(&self, duration: f32) -> Result<(), AnimationError> {
        let mut previous: Option<f32> = None;
        for keyframe in &self.keyframes {
            if !keyframe.time.is_finite() || keyframe.time < 0.0 {
                return Err(AnimationError::NonFiniteTime);
            }
            if keyframe.time > duration {
                return Err(AnimationError::KeyframeAfterDuration);
            }
            if let Some(last) = previous {
                if keyframe.time < last {
                    return Err(AnimationError::UnsortedKeyframes);
                }
                if keyframe.time == last {
                    return Err(AnimationError::DuplicateKeyframeTime);
                }
            }
            previous = Some(keyframe.time);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
            return Err(AnimationError::InvalidDuration);
        }

        for track in translation_tracks.iter().chain(scale_tracks.iter()) {
            track.validate_times(duration)?;
            if track.keyframes.iter().any(|keyframe| !keyframe.value.is_finite()) {
                return Err(AnimationError::NonFiniteValue);
            }
        }
        for track in &rotation_tracks {
            track.validate_times(duration)?;
            if track.keyframes.iter().any(|keyframe| {
                !keyframe.value.is_normalized(QUATERNION_NORM_TOLERANCE)
            }) {
                return Err(AnimationError::InvalidQuaternion);
            }
        }

        Ok(Self {
            name: name.into(),
            duration,
            translation_tracks,
            rotation_tracks,
            scale_tracks,
        })
    }

    /// Validates all track targets against a concrete skeleton before sampling.
    pub fn validate_for_skeleton(&self, skeleton: &Skeleton) -> Result<(), AnimationError> {
        for joint in self
            .translation_tracks
            .iter()
            .map(|track| track.joint)
            .chain(self.rotation_tracks.iter().map(|track| track.joint))
            .chain(self.scale_tracks.iter().map(|track| track.joint))
        {
            if joint.0 >= skeleton.len() {
                return Err(AnimationError::JointOutOfRange {
                    joint: joint.0,
                    skeleton_len: skeleton.len(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalPose {
    pub transforms: Vec<Transform>,
}

impl LocalPose {
    pub fn from_bind_pose(skeleton: &Skeleton) -> Self {
        Self {
            transforms: skeleton.joints().iter().map(|joint| joint.bind_pose).collect(),
        }
    }

    pub fn validate_for_skeleton(&self, skeleton: &Skeleton) -> Result<(), AnimationError> {
        if self.transforms.len() != skeleton.len() {
            return Err(AnimationError::PoseSkeletonMismatch {
                pose_len: self.transforms.len(),
                skeleton_len: skeleton.len(),
            });
        }
        if self.transforms.iter().any(|transform| !transform.is_valid()) {
            return Err(AnimationError::InvalidTransform);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelPose {
    pub transforms: Vec<Transform>,
}

impl ModelPose {
    pub fn from_local_pose(local_pose: &LocalPose, skeleton: &Skeleton) -> Result<Self, AnimationError> {
        local_pose.validate_for_skeleton(skeleton)?;
        let mut model_transforms = Vec::with_capacity(skeleton.len());
        for (index, joint) in skeleton.joints().iter().enumerate() {
            let local = local_pose.transforms[index];
            let model = match joint.parent {
                Some(parent) => Transform::combine(model_transforms[parent.0], local),
                None => local,
            };
            model_transforms.push(model);
        }
        Ok(Self {
            transforms: model_transforms,
        })
    }

    pub fn matrix_palette(&self, skeleton: &Skeleton) -> Result<Vec<Mat4>, AnimationError> {
        if self.transforms.len() != skeleton.len() {
            return Err(AnimationError::PoseSkeletonMismatch {
                pose_len: self.transforms.len(),
                skeleton_len: skeleton.len(),
            });
        }
        Ok(self
            .transforms
            .iter()
            .zip(skeleton.joints())
            .map(|(model_transform, joint)| {
                model_transform
                    .to_mat4()
                    .multiply(joint.inverse_bind_matrix)
            })
            .collect())
    }
}

pub fn clamp_or_loop_time(time: f32, duration: f32, looping: bool) -> Result<f32, AnimationError> {
    if !time.is_finite() {
        return Err(AnimationError::NonFiniteTime);
    }
    if !duration.is_finite() || duration < 0.0 {
        return Err(AnimationError::InvalidDuration);
    }
    if duration <= f32::EPSILON {
        return Ok(0.0);
    }
    if looping {
        Ok(time.rem_euclid(duration))
    } else {
        Ok(time.clamp(0.0, duration))
    }
}

fn sample_vec3_track(track: &Track<Vec3>, time: f32, default: Vec3) -> Vec3 {
    let Some(first) = track.keyframes.first() else {
        return default;
    };
    if track.keyframes.len() == 1 || time <= first.time {
        return first.value;
    }
    let last = track.keyframes.last().expect("non-empty track established above");
    if time >= last.time {
        return last.value;
    }
    for window in track.keyframes.windows(2) {
        if time >= window[0].time && time <= window[1].time {
            let factor = (time - window[0].time) / (window[1].time - window[0].time);
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

fn sample_quat_track(track: &Track<Quat>, time: f32, default: Quat) -> Quat {
    let Some(first) = track.keyframes.first() else {
        return default;
    };
    if track.keyframes.len() == 1 || time <= first.time {
        return first.value;
    }
    let last = track.keyframes.last().expect("non-empty track established above");
    if time >= last.time {
        return last.value;
    }
    for window in track.keyframes.windows(2) {
        if time >= window[0].time && time <= window[1].time {
            let factor = (time - window[0].time) / (window[1].time - window[0].time);
            return match track.interpolation {
                InterpolationMode::Step => window[0].value,
                InterpolationMode::Linear => window[0].value.nlerp(window[1].value, factor),
                InterpolationMode::Slerp => window[0].value.slerp(window[1].value, factor),
            };
        }
    }
    last.value
}

/// Samples a validated clip. Invalid time or joint references fail rather than being ignored.
pub fn sample_clip(
    clip: &AnimationClip,
    skeleton: &Skeleton,
    time: f32,
    looping: bool,
) -> Result<LocalPose, AnimationError> {
    clip.validate_for_skeleton(skeleton)?;
    let sample_time = clamp_or_loop_time(time, clip.duration, looping)?;
    let mut pose = LocalPose::from_bind_pose(skeleton);

    for track in &clip.translation_tracks {
        let index = track.joint.0;
        let default_value = pose.transforms[index].translation;
        pose.transforms[index].translation = sample_vec3_track(track, sample_time, default_value);
    }
    for track in &clip.rotation_tracks {
        let index = track.joint.0;
        let default_value = pose.transforms[index].rotation;
        pose.transforms[index].rotation = sample_quat_track(track, sample_time, default_value);
    }
    for track in &clip.scale_tracks {
        let index = track.joint.0;
        let default_value = pose.transforms[index].scale;
        pose.transforms[index].scale = sample_vec3_track(track, sample_time, default_value);
    }
    Ok(pose)
}

pub fn blend_poses(
    pose_a: &LocalPose,
    pose_b: &LocalPose,
    amount: f32,
) -> Result<LocalPose, AnimationError> {
    if !amount.is_finite() {
        return Err(AnimationError::InvalidBlendWeight);
    }
    if pose_a.transforms.len() != pose_b.transforms.len() {
        return Err(AnimationError::PoseLengthMismatch {
            left: pose_a.transforms.len(),
            right: pose_b.transforms.len(),
        });
    }
    let alpha = amount.clamp(0.0, 1.0);
    let transforms = pose_a
        .transforms
        .iter()
        .zip(&pose_b.transforms)
        .map(|(a, b)| Transform {
            translation: a.translation.lerp(b.translation, alpha),
            rotation: a.rotation.slerp(b.rotation, alpha),
            scale: a.scale.lerp(b.scale, alpha),
        })
        .collect();
    Ok(LocalPose { transforms })
}

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

    pub fn advance(&mut self, delta_seconds: f32) -> Result<(), AnimationError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(AnimationError::InvalidDeltaTime);
        }
        if !self.speed.is_finite() {
            return Err(AnimationError::InvalidPlaybackRate);
        }
        if !self.time.is_finite() {
            return Err(AnimationError::NonFiniteTime);
        }
        if !self.playing {
            return Ok(());
        }
        if let Some(clip) = &self.clip {
            let next = self.time + delta_seconds * self.speed;
            self.time = clamp_or_loop_time(next, clip.duration, self.looping)?;
        }
        Ok(())
    }
}

pub fn update_animation_players(world: &mut World, delta_seconds: f32) -> Result<(), AnimationError> {
    for (_entity, player) in world.iter_mut::<AnimationPlayer>() {
        player.advance(delta_seconds)?;
    }
    Ok(())
}

/// Contracts for EEFP/VPAE research. They are deliberately deterministic and transaction-oriented.
pub mod adaptive {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ProgressiveDecision {
        Continue,
        Accept,
        Recover,
    }

    /// Transactional adaptive-domain contract. Hard validation happens before mutation;
    /// verification happens after mutation; rollback must restore an admissible checkpoint.
    pub trait AdaptiveDomain {
        type State;
        type Checkpoint;
        type Observation;
        type Plan;
        type Error;

        fn checkpoint(&self, state: &Self::State) -> Self::Checkpoint;
        fn observe(&self, state: &Self::State) -> Self::Observation;
        fn propose(&mut self, observation: &Self::Observation) -> Result<Self::Plan, Self::Error>;
        fn validate(&self, state: &Self::State, plan: &Self::Plan) -> Result<(), Self::Error>;
        fn apply(&mut self, state: &mut Self::State, plan: &Self::Plan) -> Result<(), Self::Error>;
        fn verify(
            &self,
            checkpoint: &Self::Checkpoint,
            state: &Self::State,
            plan: &Self::Plan,
        ) -> Result<(), Self::Error>;
        fn commit(&mut self, state: &mut Self::State, plan: &Self::Plan) -> Result<(), Self::Error>;
        fn rollback(
            &mut self,
            state: &mut Self::State,
            checkpoint: Self::Checkpoint,
            plan: &Self::Plan,
        ) -> Result<(), Self::Error>;
    }

    /// VPAE-style evaluator. `Accept` is impossible while a hard invariant is false.
    pub trait ProgressiveEvaluator {
        type State;
        type Observation;
        type Error;

        fn observe(&self, state: &Self::State) -> Self::Observation;
        fn hard_invariants_hold(&self, state: &Self::State) -> bool;
        fn decide(
            &mut self,
            state: &Self::State,
            observation: &Self::Observation,
        ) -> Result<ProgressiveDecision, Self::Error>;
    }

    pub fn evaluate_progress<E: ProgressiveEvaluator>(
        evaluator: &mut E,
        state: &E::State,
    ) -> Result<ProgressiveDecision, E::Error> {
        if !evaluator.hard_invariants_hold(state) {
            return Ok(ProgressiveDecision::Recover);
        }
        let observation = evaluator.observe(state);
        evaluator.decide(state, &observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_joint_skeleton() -> Skeleton {
        Skeleton::new(vec![Joint {
            name: "root".to_owned(),
            parent: None,
            bind_pose: Transform::IDENTITY,
            inverse_bind_matrix: Mat4::IDENTITY,
        }])
        .expect("valid skeleton")
    }

    #[test]
    fn skeleton_creation_and_validation() {
        let root = Joint {
            name: "root".to_owned(),
            parent: None,
            bind_pose: Transform::IDENTITY,
            inverse_bind_matrix: Mat4::IDENTITY,
        };
        let spine = Joint {
            name: "spine".to_owned(),
            parent: Some(JointId(0)),
            bind_pose: Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -1.0, 0.0)),
        };
        assert_eq!(Skeleton::new(vec![root, spine]).expect("valid").len(), 2);
    }

    #[test]
    fn duplicate_and_non_finite_keyframes_are_rejected() {
        let duplicate = Track {
            joint: JointId(0),
            interpolation: InterpolationMode::Linear,
            keyframes: vec![
                Keyframe { time: 0.0, value: Vec3::ZERO },
                Keyframe { time: 0.0, value: Vec3::ONE },
            ],
        };
        assert_eq!(
            AnimationClip::new("bad", 1.0, vec![duplicate], vec![], vec![]),
            Err(AnimationError::DuplicateKeyframeTime)
        );

        let non_finite = Track {
            joint: JointId(0),
            interpolation: InterpolationMode::Linear,
            keyframes: vec![Keyframe {
                time: 0.0,
                value: Vec3::new(f32::NAN, 0.0, 0.0),
            }],
        };
        assert_eq!(
            AnimationClip::new("bad", 1.0, vec![non_finite], vec![], vec![]),
            Err(AnimationError::NonFiniteValue)
        );
    }

    #[test]
    fn invalid_joint_is_not_silently_ignored() {
        let skeleton = one_joint_skeleton();
        let track = Track {
            joint: JointId(7),
            interpolation: InterpolationMode::Linear,
            keyframes: vec![Keyframe { time: 0.0, value: Vec3::ZERO }],
        };
        let clip = AnimationClip::new("bad joint", 1.0, vec![track], vec![], vec![]).expect("clip shape");
        assert_eq!(
            sample_clip(&clip, &skeleton, 0.0, false),
            Err(AnimationError::JointOutOfRange { joint: 7, skeleton_len: 1 })
        );
    }

    #[test]
    fn animation_clip_sampling_and_blending() {
        let skeleton = one_joint_skeleton();
        let track = Track {
            joint: JointId(0),
            interpolation: InterpolationMode::Linear,
            keyframes: vec![
                Keyframe { time: 0.0, value: Vec3::ZERO },
                Keyframe { time: 1.0, value: Vec3::new(10.0, 0.0, 0.0) },
            ],
        };
        let clip = AnimationClip::new("walk", 1.0, vec![track], vec![], vec![]).expect("clip");
        let half = sample_clip(&clip, &skeleton, 0.5, false).expect("sample");
        let full = sample_clip(&clip, &skeleton, 1.0, false).expect("sample");
        assert!((half.transforms[0].translation.x - 5.0).abs() < 1e-4);
        let blended = blend_poses(&half, &full, 0.5).expect("blend");
        assert!((blended.transforms[0].translation.x - 7.5).abs() < 1e-4);
    }

    #[test]
    fn pose_length_mismatch_fails_closed() {
        let a = LocalPose { transforms: vec![Transform::IDENTITY] };
        let b = LocalPose { transforms: vec![] };
        assert_eq!(
            blend_poses(&a, &b, 0.5),
            Err(AnimationError::PoseLengthMismatch { left: 1, right: 0 })
        );
    }

    #[test]
    fn non_finite_runtime_time_is_rejected() {
        assert_eq!(clamp_or_loop_time(f32::NAN, 1.0, true), Err(AnimationError::NonFiniteTime));
        let mut player = AnimationPlayer::default();
        assert_eq!(player.advance(f32::NAN), Err(AnimationError::InvalidDeltaTime));
    }

    #[test]
    fn model_pose_and_matrix_palette_computation() {
        let root = Joint {
            name: "root".to_owned(),
            parent: None,
            bind_pose: Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -1.0, 0.0)),
        };
        let child = Joint {
            name: "child".to_owned(),
            parent: Some(JointId(0)),
            bind_pose: Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
            inverse_bind_matrix: Mat4::translation(Vec3::new(0.0, -3.0, 0.0)),
        };
        let skeleton = Skeleton::new(vec![root, child]).expect("skeleton");
        let local_pose = LocalPose::from_bind_pose(&skeleton);
        let model_pose = ModelPose::from_local_pose(&local_pose, &skeleton).expect("model pose");
        assert_eq!(model_pose.transforms[1].translation.y, 3.0);
        assert_eq!(model_pose.matrix_palette(&skeleton).expect("palette").len(), 2);
    }
}