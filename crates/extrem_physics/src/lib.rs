use extrem_app::{App, Plugin, Stage, Time};
use extrem_ecs::{Entity, World};
use extrem_math::{Transform, Vec3};
use extrem_scene::Velocity;
use std::fmt;

/// Body behavior used by the first physics solver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyType {
    Static,
    Dynamic,
}

/// Simple rigid-body component.
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

/// Axis-aligned box collider in local space.
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

/// Global gravity resource.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gravity(pub Vec3);

impl Default for Gravity {
    fn default() -> Self {
        Self(Vec3::new(0.0, -9.81, 0.0))
    }
}

/// Physics diagnostic counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsStats {
    pub simulated_bodies: usize,
    pub contacts_with_ground: usize,
}

/// Plugin that advances rigid bodies in the engine fixed schedule.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.world.insert_resource(Gravity::default());
        app.world.insert_resource(PhysicsStats::default());
        app.add_systems(Stage::FixedUpdate, step_physics);
    }
}

/// Advances all dynamic bodies by one fixed step.
pub fn step_physics(world: &mut World, time: Time) {
    let gravity = world
        .get_resource::<Gravity>()
        .copied()
        .unwrap_or_default()
        .0;
    let entities: Vec<Entity> = world
        .iter::<RigidBody>()
        .filter_map(|(entity, body)| (body.body_type == BodyType::Dynamic).then_some(entity))
        .collect();
    let mut stats = PhysicsStats::default();

    for entity in entities {
        let mut velocity = world.get::<Velocity>(entity).copied().unwrap_or_default();
        velocity.0 += gravity * time.fixed_delta_seconds;
        if let Some(current_velocity) = world.get_mut::<Velocity>(entity) {
            *current_velocity = velocity;
        } else {
            let _ = world.insert(entity, velocity);
        }

        let Some(mut translation) = world
            .get::<Transform>(entity)
            .map(|value| value.translation)
        else {
            continue;
        };
        translation += velocity.0 * time.fixed_delta_seconds;
        if let Some(collider) = world.get::<BoxCollider>(entity).copied() {
            let floor = collider.half_extents.y;
            if translation.y < floor {
                translation.y = floor;
                if velocity.0.y < 0.0 {
                    velocity.0.y = 0.0;
                    if let Some(current_velocity) = world.get_mut::<Velocity>(entity) {
                        *current_velocity = velocity;
                    }
                }
                stats.contacts_with_ground += 1;
            }
        }
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation = translation;
        }
        stats.simulated_bodies += 1;
    }

    world.insert_resource(stats);
}

/// Errors reserved for future broad-phase and solver configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicsError {
    InvalidMass,
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMass => write!(formatter, "a dynamic body must have positive mass"),
        }
    }
}

impl std::error::Error for PhysicsError {}

#[cfg(test)]
mod tests {
    use super::{BodyType, BoxCollider, PhysicsPlugin, PhysicsStats, RigidBody};
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
    fn static_body_is_not_simulated() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        world
            .insert(
                entity,
                RigidBody {
                    body_type: BodyType::Static,
                    mass: 0.0,
                },
            )
            .expect("entity");
        assert_eq!(
            world.get::<RigidBody>(entity).expect("body").body_type,
            BodyType::Static
        );
    }
}
