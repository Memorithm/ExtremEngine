use extrem_app::{App, Plugin, Stage, Time};
use extrem_ecs::{Entity, World};
use extrem_math::{Transform, Vec3};
use extrem_scene::Velocity;
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
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            mass: 1.0,
        }
    }
}

impl RigidBody {
    pub fn dynamic(mass: f32) -> Result<Self, PhysicsError> {
        let body = Self {
            body_type: BodyType::Dynamic,
            mass,
        };
        body.validate()?;
        Ok(body)
    }

    pub fn validate(self) -> Result<(), PhysicsError> {
        match self.body_type {
            BodyType::Dynamic if !self.mass.is_finite() || self.mass <= 0.0 => {
                Err(PhysicsError::InvalidMass)
            }
            BodyType::Static if !self.mass.is_finite() || self.mass < 0.0 => {
                Err(PhysicsError::InvalidMass)
            }
            _ => Ok(()),
        }
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
    let mut entities: Vec<Entity> = world
        .iter::<RigidBody>()
        .filter_map(|(entity, body)| (body.body_type == BodyType::Dynamic).then_some(entity))
        .collect();
    entities.sort_unstable();

    for entity in entities {
        let Some(body) = world.get::<RigidBody>(entity).copied() else {
            continue;
        };
        if body.validate().is_err() {
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
        let mut next_translation = transform.translation + next_velocity.0 * fixed_delta;
        if !next_velocity.0.is_finite() || !next_translation.is_finite() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }

        if let Some(collider) = collider {
            let floor = collider.half_extents.y;
            if next_translation.y < floor {
                next_translation.y = floor;
                if next_velocity.0.y < 0.0 {
                    next_velocity.0.y = 0.0;
                }
                stats.contacts_with_ground = stats.contacts_with_ground.saturating_add(1);
            }
        }

        // Commit only after the complete candidate state passed all validation.
        if let Some(current_velocity) = world.get_mut::<Velocity>(entity) {
            *current_velocity = next_velocity;
        } else if world.insert(entity, next_velocity).is_err() {
            stats.rejected_bodies = stats.rejected_bodies.saturating_add(1);
            continue;
        }
        if let Some(current_transform) = world.get_mut::<Transform>(entity) {
            current_transform.translation = next_translation;
        }
        stats.simulated_bodies = stats.simulated_bodies.saturating_add(1);
    }

    world.insert_resource(stats);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicsError {
    InvalidMass,
    InvalidCollider,
    InvalidGravity,
    InvalidTimestep,
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMass => write!(formatter, "a dynamic body must have finite positive mass"),
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
    use super::{BodyType, BoxCollider, PhysicsError, PhysicsPlugin, PhysicsStats, RigidBody};
    use extrem_app::App;
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};

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
    }

    #[test]
    fn static_body_may_have_zero_mass() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let body = RigidBody {
            body_type: BodyType::Static,
            mass: 0.0,
        };
        assert_eq!(body.validate(), Ok(()));
        world.insert(entity, body).expect("entity");
        assert_eq!(world.get::<RigidBody>(entity), Some(&body));
    }
}
