use extrem_ecs::{Entity, World};

use crate::{Parent, Visibility};

/// True when the entity is visible and no ancestor is `Visibility(false)`.
pub fn is_hierarchically_visible(world: &World, entity: Entity) -> bool {
    use std::collections::HashSet;

    if !world.contains(entity) {
        return false;
    }
    let mut current = Some(entity);
    let mut visited = HashSet::new();
    while let Some(node) = current {
        if !visited.insert(node) {
            break;
        }
        if world.get::<Visibility>(node).is_some_and(|value| !value.0) {
            return false;
        }
        current = world.get::<Parent>(node).map(|parent| parent.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_hierarchically_visible;
    use crate::{Scene, Visibility};
    use extrem_ecs::World;
    use extrem_math::Transform;

    #[test]
    fn hidden_ancestor_hides_child() {
        let mut world = World::new();
        let mut scene = Scene::new("vis");
        let parent = scene
            .spawn_entity(&mut world, "parent", Transform::IDENTITY)
            .expect("parent");
        let child = scene
            .spawn_child(&mut world, parent, "child", Transform::IDENTITY)
            .expect("child");
        world.insert(parent, Visibility(false)).expect("hide");
        assert!(!is_hierarchically_visible(&world, child));
        assert!(!is_hierarchically_visible(&world, parent));
    }
}
