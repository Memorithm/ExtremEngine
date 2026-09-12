use extrem_app::{App, Plugin, Stage, Time};
use extrem_ecs::{Entity, World};
use extrem_math::{Transform, Vec3};
use extrem_scene::{Parent, Velocity};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyType {
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBody {
    pub body_type: BodyType,
    pub mass: f32,
    /// Per-second linear damping in `[0, +inf)`. `0` keeps the previous integrator.
    pub linear_damping: f32,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            mass: 1.0,
            linear_damping: 0.0,
        }
    }
}

impl RigidBody {
    pub fn dynamic(mass: f32) -> Result<Self, PhysicsError> {
        let body = Self {
            body_type: BodyType::Dynamic,
            mass,
            linear_damping: 0.0,
        };
        body.validate()?;
        Ok(body)
    }

    pub fn validate(self) -> Result<(), PhysicsError> {
        match self.body_type {
            BodyType::Dynamic if !self.mass.is_finite() || self.mass <= 0.0 => {
                return Err(PhysicsError::InvalidMass);
            }
            BodyType::Static if !self.mass.is_finite() || self.mass < 0.0 => {
                return Err(PhysicsError::InvalidMass);
            }
            _ => {}
        }
        if !self.linear_damping.is_finite() || self.linear_damping < 0.0 {
            return Err(PhysicsError::InvalidDamping);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxCollider {
    pub half_extents: Vec3,
}

impl Default for BoxCollider {
    fn default() -> Self {
        Self {
            half_extents: Vec3::new(0.5, 0.5, 0.5),
        }
    }
}

impl BoxCollider {
    pub fn validate(self) -> Result<(), PhysicsError> {
        if !self.half_extents.is_finite()
            || self.half_extents.x <= 0.0
            || self.half_extents.y <= 0.0
            || self.half_extents.z <= 0.0
        {
            return Err(PhysicsError::InvalidCollider);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gravity(pub Vec3);

impl Default for Gravity {
    fn default() -> Self {
        Self(Vec3::new(0.0, -9.81, 0.0))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsStats {
    pub simulated_bodies: usize,
    pub contacts_with_ground: usize,
    pub contacts_with_bodies: usize,
    pub rejected_bodies: usize,
}

/// Last global physics fault. Invalid individual bodies are counted in [`PhysicsStats::rejected_bodies`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsFault(pub Option<PhysicsError>);

#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.world.insert_resource(Gravity::default());
        app.world.insert_resource(PhysicsStats::default());
        app.world.insert_resource(PhysicsFault::default());
        app.add_systems(Stage::FixedUpdate, step_physics);
    }
}

#[derive(Clone, Copy)]
struct ColliderPose {
    entity: Entity,
    translation: Vec3,
    half_extents: Vec3,
}

/// Pushes `translation` out of `other` along the smallest overlap axis.
/// Incoming velocity on that axis is cancelled.
fn resolve_aabb(
    translation: &mut Vec3,
    velocity: &mut Vec3,
    half: Vec3,
    other: Vec3,
    other_half: Vec3,
) -> bool {
    let delta = Vec3::new(
        translation.x - other.x,
        translation.y - other.y,
        translation.z - other.z,
    );
    let overlap = Vec3::new(
        half.x + other_half.x - delta.x.abs(),
        half.y + other_half.y - delta.y.abs(),
        half.z + other_half.z - delta.z.abs(),
    );
    if !overlap.is_finite() || overlap.x <= 0.0 || overlap.y <= 0.0 || overlap.z <= 0.0 {
        return false;
    }

    if overlap.x <= overlap.y && overlap.x <= overlap.z {
        let sign = if delta.x == 0.0 { 1.0 } else { delta.x.signum() };
        translation.x += overlap.x * sign;
        if velocity.x * sign < 0.0 {
            velocity.x = 0.0;
        }
    } else if overlap.y <= overlap.z {
        let sign = if delta.y == 0.0 { 1.0 } else { delta.y.signum() };
        translation.y += overlap.y * sign;
        if velocity.y * sign < 0.0 {
            velocity.y = 0.0;
        }
    } else {
        let sign = if delta.z == 0.0 { 1.0 } else { delta.z.signum() };
        translation.z += overlap.z * sign;
        if velocity.z * sign < 0.0 {
            velocity.z = 0.0;
        }
    }
    translation.is_finite() && velocity.is_finite()
}

fn collect_collider_poses(world: &World) -> Vec<ColliderPose> {
    let mut poses: Vec<ColliderPose> = world
        .iter::<BoxCollider>()
        .filter_map(|(entity, collider)| {
            // Local transforms are not a common collision space. Until the reference solver
            // consumes GlobalTransform, parented colliders are intentionally unsupported.
            if world.get::<Parent>(entity).is_some() {
                return None;
            }
            collider.validate().ok()?;
            let transform = world.get::<Transform>(entity).copied()?;
            if !transform.is_valid() {
                return None;
            }
            Some(ColliderPose {
                entity,
                translation: transform.translation,
                half_extents: collider.half_extents,
            })
        })
        .collect();
    poses.sort_by_key(|pose| pose.entity);
    poses
}

/// Advances the minimal reference physics system. Invalid state is rejected before mutation.
pub fn step_physics(world: &mut World, time: Time) {
    let mut stats = PhysicsStats::default();
    let fixed_delta = time.fixed_delta_seconds;
    if !fixed_delta.is_finite() || fixed_delta <= 0.0 {
        world.insert_resource(stats);
        world.insert_resource(PhysicsFault(Some(PhysicsError::InvalidTimestep)));
        return;
    }

    let gravity = world
        .get_resource::<Gravity>()
        .copied()
        .unwrap_or_default()
        .0;
    if !gravity.is_finite() {
        world.insert_resource(stats);
        world.insert_resource(PhysicsFault(Some(PhysicsError::InvalidGravity)));
        return;
    }

    world.insert_resource(PhysicsFault(None));
    let mut poses = collect_collider_poses(world);
    let mut entities: Vec<Entity> = world
        .iter::<RigidBody>()
        .filter_map(|(entity, body)| (body.body_type == BodyType::Dynamic).then_some(entity))
        .collect();
    entities.sort_unstable();

    for entity in entities {
        let Some(body) = world.get::<RigidBody>(entity).copied() else {
            continue;
        };
        if body.validate().is_err() || world.get::<Parent>(entity).is_some() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        let Some(transform) = world.get::<Transform>(entity).copied() else {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        };
        if !transform.is_valid() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        let collider = world.get::<BoxCollider>(entity).copied();
        if collider.is_some_and(|value| value.validate().is_err()) {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        let previous_velocity = world.get::<Velocity>(entity).copied().unwrap_or_default();
        if !previous_velocity.0.is_finite() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        let mut next_velocity = previous_velocity;
        next_velocity.0 += gravity * fixed_delta;
        let damping = (1.0 - body.linear_damping * fixed_delta).clamp(0.0, 1.0);
        next_velocity.0 *= damping;
        let mut next_translation = transform.translation + next_velocity.0 * fixed_delta;
        if !next_velocity.0.is_finite() || !next_translation.is_finite() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        let mut body_contacts = 0usize;
        if let Some(collider) = collider {
            let floor = collider.half_extents.y;
            if next_translation.y < floor {
                next_translation.y = floor;
                if next_velocity.0.y < 0.0 {
                    next_velocity.0.y = 0.0;
                }
                stats.contacts_with_ground = stats.contacts_with_ground.saturating_add(1);
            }

            for pose in &poses {
                if pose.entity == entity {
                    continue;
                }
                if resolve_aabb(
                    &mut next_translation,
                    &mut next_velocity.0,
                    collider.half_extents,
                    pose.translation,
                    pose.half_extents,
                ) {
                    body_contacts = body_contacts.saturating_add(1);
                }
            }
        }

        if !next_velocity.0.is_finite() || !next_translation.is_finite() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        if let Some(current_velocity) = world.get_mut::<Velocity>(entity) {
            *current_velocity = next_velocity;
        } else if world.insert(entity, next_velocity).is_err() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }
        if let Some(current_transform) = world.get_mut::<Transform>(entity) {
            current_transform.translation = next_translation;
        }
        if let Some(pose) = poses.iter_mut().find(|pose| pose.entity == entity) {
            pose.translation = next_translation;
        }
        stats.contacts_with_bodies = stats.contacts_with_bodies.saturating_add(body_contacts);
        stats.simulated_bodies = stats.simulated_bodies.saturating_add(1);
    }

    world.insert_resource(stats);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicsError {
    InvalidMass,
    InvalidDamping,
    InvalidCollider,
    InvalidGravity,
    InvalidTimestep,
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMass => write!(formatter, "a dynamic body must have finite positive mass"),
            Self::InvalidDamping => {
                write!(formatter, "linear damping must be finite and non-negative")
            }
            Self::InvalidCollider => {
                write!(formatter, "box half-extents must be finite and positive")
            }
            Self::InvalidGravity => write!(formatter, "gravity must contain only finite values"),
            Self::InvalidTimestep => {
                write!(formatter, "physics timestep must be finite and positive")
            }
        }
    }
}

impl std::error::Error for PhysicsError {}

#[cfg(test)]
mod tests {
    use super::{
        BodyType, BoxCollider, PhysicsError, PhysicsPlugin, PhysicsStats, RigidBody,
    };
    use extrem_app::App;
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};
    use extrem_scene::{set_parent, Parent, Velocity};

    #[test]
    fn dynamic_body_falls_and_stops_on_ground() {
        let mut app = App::new();
        app.add_plugin(PhysicsPlugin);
        let entity = app.world_mut().spawn_empty();
        app.world_mut()
            .insert(entity, RigidBody::default())
            .expect("entity");
        app.world_mut()
            .insert(entity, BoxCollider::default())
            .expect("entity");
        app.world_mut()
            .insert(
                entity,
                Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
            )
            .expect("entity");

        app.run_for(60, 1.0 / 60.0);
        assert!(
            app.world()
                .get::<Transform>(entity)
                .expect("transform")
                .translation
                .y
                >= 0.5
        );
        assert!(
            app.world()
                .get_resource::<PhysicsStats>()
                .expect("stats")
                .simulated_bodies
                > 0
        );
    }

    #[test]
    fn dynamic_body_rests_on_a_static_platform() {
        let mut app = App::new();
        app.add_plugin(PhysicsPlugin);
        let platform = app.world_mut().spawn_empty();
        app.world_mut()
            .insert(
                platform,
                RigidBody {
                    body_type: BodyType::Static,
                    mass: 0.0,
                    linear_damping: 0.0,
                },
            )
            .expect("static");
        app.world_mut()
            .insert(platform, BoxCollider::default())
            .expect("collider");
        app.world_mut()
            .insert(
                platform,
                Transform::from_translation(Vec3::new(0.0, 0.5, 0.0)),
            )
            .expect("transform");

        let falling = app.world_mut().spawn_empty();
        app.world_mut()
            .insert(falling, RigidBody::default())
            .expect("dynamic");
        app.world_mut()
            .insert(falling, BoxCollider::default())
            .expect("collider");
        app.world_mut()
            .insert(
                falling,
                Transform::from_translation(Vec3::new(0.0, 3.0, 0.0)),
            )
            .expect("transform");

        app.run_for(90, 1.0 / 60.0);
        let height = app
            .world()
            .get::<Transform>(falling)
            .expect("transform")
            .translation
            .y;
        assert!(height >= 1.49);
        assert_eq!(
            app.world()
                .get::<Transform>(platform)
                .expect("platform")
                .translation
                .y,
            0.5
        );
        assert!(
            app.world()
                .get_resource::<PhysicsStats>()
                .expect("stats")
                .contacts_with_bodies
                > 0
        );
    }

    #[test]
    fn parented_dynamic_body_is_rejected_until_global_space_is_supported() {
        let mut app = App::new();
        app.add_plugin(PhysicsPlugin);
        let parent = app.world_mut().spawn(Transform::IDENTITY);
        let child = app.world_mut().spawn(Transform::IDENTITY);
        app.world_mut()
            .insert(child, RigidBody::default())
            .expect("body");
        app.world_mut()
            .insert(child, BoxCollider::default())
            .expect("collider");
        set_parent(app.world_mut(), child, parent).expect("parent");
        assert_eq!(app.world().get::<Parent>(child), Some(&Parent(parent)));

        app.update(1.0 / 60.0);
        let stats = app.world().get_resource::<PhysicsStats>().expect("stats");
        assert_eq!(stats.simulated_bodies, 0);
        assert_eq!(stats.rejected_bodies, 1);
        assert_eq!(app.world().get::<Transform>(child), Some(&Transform::IDENTITY));
    }

    #[test]
    fn invalid_dynamic_mass_is_rejected_before_simulation() {
        let mut app = App::new();
        app.add_plugin(PhysicsPlugin);
        let entity = app.world_mut().spawn(Transform::IDENTITY);
        app.world_mut()
            .insert(
                entity,
                RigidBody {
                    body_type: BodyType::Dynamic,
                    mass: f32::NAN,
                    linear_damping: 0.0,
                },
            )
            .expect("body");
        app.update(1.0 / 60.0);
        let stats = app.world().get_resource::<PhysicsStats>().expect("stats");
        assert_eq!(stats.simulated_bodies, 0);
        assert_eq!(stats.rejected_bodies, 1);
        assert_eq!(
            app.world().get::<Transform>(entity),
            Some(&Transform::IDENTITY)
        );
    }

    #[test]
    fn constructors_and_colliders_validate_numeric_contracts() {
        assert_eq!(RigidBody::dynamic(0.0), Err(PhysicsError::InvalidMass));
        assert_eq!(RigidBody::dynamic(1.0).expect("valid").mass, 1.0);
        assert_eq!(
            BoxCollider {
                half_extents: Vec3::new(1.0, -1.0, 1.0),
            }
            .validate(),
            Err(PhysicsError::InvalidCollider)
        );
        let invalid = RigidBody {
            linear_damping: -1.0,
            ..RigidBody::default()
        };
        assert_eq!(invalid.validate(), Err(PhysicsError::InvalidDamping));
    }

    #[test]
    fn static_body_may_have_zero_mass() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let body = RigidBody {
            body_type: BodyType::Static,
            mass: 0.0,
            linear_damping: 0.0,
        };
        assert_eq!(body.validate(), Ok(()));
        world.insert(entity, body).expect("entity");
        assert_eq!(world.get::<RigidBody>(entity), Some(&body));
    }

    #[test]
    fn linear_damping_reduces_horizontal_speed() {
        let mut app = App::new();
        app.add_plugin(PhysicsPlugin);
        app.world_mut().insert_resource(super::Gravity(Vec3::ZERO));
        let entity = app.world_mut().spawn_empty();
        app.world_mut()
            .insert(
                entity,
                RigidBody {
                    body_type: BodyType::Dynamic,
                    mass: 1.0,
                    linear_damping: 4.0,
                },
            )
            .expect("body");
        app.world_mut()
            .insert(entity, Transform::IDENTITY)
            .expect("transform");
        app.world_mut()
            .insert(entity, Velocity(Vec3::new(10.0, 0.0, 0.0)))
            .expect("velocity");
        app.update(1.0 / 60.0);
        let speed = app.world().get::<Velocity>(entity).expect("velocity").0.x;
        assert!(speed > 0.0 && speed < 10.0);
    }
}
