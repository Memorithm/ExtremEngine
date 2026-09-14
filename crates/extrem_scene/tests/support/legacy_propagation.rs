//! Frozen propagation body from a00b6e513162d9a82473dfa5fe041e6cda465756.
//! Only the function name/import paths differ; never linked into the product library.
use extrem_ecs::World;
use extrem_math::Transform;
use extrem_scene::{Children, GlobalTransform, Parent};
use std::collections::HashSet;

pub fn legacy_propagate_transforms(world: &mut World) {
    let roots: Vec<_> = world
        .iter::<Transform>()
        .filter_map(|(entity, transform)| {
            world
                .get::<Parent>(entity)
                .is_none()
                .then_some((entity, *transform))
        })
        .collect();

    let mut pending = roots;
    let mut visited = HashSet::new();
    while let Some((entity, parent_global)) = pending.pop() {
        if !visited.insert(entity) {
            continue;
        }
        if let Some(global) = world.get_mut::<GlobalTransform>(entity) {
            global.0 = parent_global;
        } else if world.contains(entity) {
            let _ = world.insert(entity, GlobalTransform(parent_global));
        }
        let children: Vec<_> = world
            .get::<Children>(entity)
            .map_or_else(Vec::new, |children| children.0.clone());
        for child in children {
            if let Some(local) = world.get::<Transform>(child).copied() {
                pending.push((child, Transform::combine(parent_global, local)));
            }
        }
    }
}
