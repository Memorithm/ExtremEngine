use std::cmp::Reverse;
use std::collections::HashSet;

use extrem_ecs::{Entity, World};
use extrem_math::{Mat4, Transform};
use extrem_render::RenderCommand;
use extrem_scene::{Camera, CameraPriority, GlobalTransform, is_hierarchically_visible};

/// Picks the active camera with the highest priority, then the lowest entity id.
pub fn select_camera(world: &World, aspect: f32) -> Option<(Entity, Mat4)> {
    world
        .iter::<Camera>()
        .filter(|(_, camera)| camera.active)
        .filter_map(|(entity, camera)| {
            let transform = world
                .get::<GlobalTransform>(entity)
                .map(|global| global.0)
                .or_else(|| world.get::<Transform>(entity).copied())?;
            let priority = world
                .get::<CameraPriority>(entity)
                .copied()
                .unwrap_or_default();
            Some((priority, entity, camera.view_projection(transform, aspect)))
        })
        .max_by_key(|(priority, entity, _)| (*priority, Reverse(*entity)))
        .map(|(_, entity, matrix)| (entity, matrix))
}

/// Deterministic transform extraction that skips hierarchically hidden entities.
pub fn extract_transforms(world: &World) -> Vec<RenderCommand> {
    let mut commands: Vec<(Entity, RenderCommand)> = world
        .iter::<GlobalTransform>()
        .filter(|(entity, _)| is_hierarchically_visible(world, *entity))
        .map(|(entity, transform)| {
            (
                entity,
                RenderCommand::Transform {
                    entity,
                    translation: transform.0.translation,
                },
            )
        })
        .collect();

    let extracted: HashSet<Entity> = commands.iter().map(|(entity, _)| *entity).collect();
    commands.extend(
        world
            .iter::<Transform>()
            .filter(|(entity, _)| {
                !extracted.contains(entity) && is_hierarchically_visible(world, *entity)
            })
            .map(|(entity, transform)| {
                (
                    entity,
                    RenderCommand::Transform {
                        entity,
                        translation: transform.translation,
                    },
                )
            }),
    );
    commands.sort_by_key(|(entity, _)| *entity);
    commands.into_iter().map(|(_, command)| command).collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_transforms, select_camera};
    use extrem_ecs::World;
    use extrem_math::Transform;
    use extrem_render::RenderCommand;
    use extrem_scene::{Camera, CameraPriority, Visibility};

    #[test]
    fn higher_priority_camera_wins() {
        let mut world = World::new();
        let low = world.spawn(Transform::IDENTITY);
        let high = world.spawn(Transform::IDENTITY);
        world.insert(low, Camera::default()).expect("cam");
        world.insert(high, Camera::default()).expect("cam");
        world.insert(high, CameraPriority(10)).expect("priority");
        let selected = select_camera(&world, 1.0).expect("camera");
        assert_eq!(selected.0, high);
        assert!(low < high);
    }

    #[test]
    fn hidden_entities_are_not_extracted() {
        let mut world = World::new();
        let visible = world.spawn(Transform::IDENTITY);
        let hidden = world.spawn(Transform::IDENTITY);
        world.insert(hidden, Visibility(false)).expect("hide");
        let commands = extract_transforms(&world);
        assert_eq!(commands.len(), 1);
        match commands[0] {
            RenderCommand::Transform { entity, .. } => assert_eq!(entity, visible),
            _ => panic!("expected transform command"),
        }
    }
}
