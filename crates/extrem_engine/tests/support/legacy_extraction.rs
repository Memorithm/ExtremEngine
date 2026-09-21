//! Frozen camera/translation extraction from Engine::tick at
//! 9fe9b2a13ec4b4a0df19e8f3ddcafdbeff06a0da. Only self accesses are parameterized.
//! Not compiled into the product library.
use extrem_ecs::World;
use extrem_math::Transform;
use extrem_render::{RenderBackend, RenderCommand};
use extrem_scene::{Camera, GlobalTransform};
use std::collections::HashSet;

pub fn legacy_submit<R: RenderBackend>(world: &World, aspect: f32, renderer: &mut R) {
    let active_camera = world
        .iter::<Camera>()
        .filter(|(_, camera)| camera.active)
        .filter_map(|(entity, camera)| {
            let transform = world
                .get::<GlobalTransform>(entity)
                .map(|global| global.0)
                .or_else(|| world.get::<Transform>(entity).copied())?;
            Some((
                entity,
                camera.view_projection(transform, aspect),
                transform.translation,
            ))
        })
        .min_by_key(|(entity, _, _)| *entity);

    if let Some((entity, view_projection, world_position)) = active_camera {
        renderer.submit(RenderCommand::SetCamera {
            entity,
            view_projection,
            world_position,
        });
    }

    let global_entities: HashSet<_> = world
        .iter::<GlobalTransform>()
        .map(|(entity, _)| entity)
        .collect();
    let mut commands: Vec<_> = world
        .iter::<GlobalTransform>()
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
    commands.extend(
        world
            .iter::<Transform>()
            .filter(|(entity, _)| !global_entities.contains(entity))
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
    for (_, command) in commands {
        renderer.submit(command);
    }
}
