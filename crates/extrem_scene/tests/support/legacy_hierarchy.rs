// Frozen pre-optimization reference from ExtremEngine commit
// 9dc2a79020f3bd000e985db807cbc474942e52d5, extrem_scene/src/lib.rs.
// Kept only in tests/examples; never used by the production library.
use extrem_ecs::{World, WorldError};
use extrem_scene::{Children, HierarchyError, Parent};
use std::collections::HashSet;

pub fn legacy_validate_hierarchy(world: &World) -> Result<(), HierarchyError> {
    for (entity, parent) in world.iter::<Parent>() {
        let parent_entity = parent.0;
        if !world.contains(parent_entity) {
            return Err(WorldError::EntityNotFound(parent_entity).into());
        }
        if entity == parent_entity {
            return Err(HierarchyError::SelfParent(entity));
        }
        let parent_has_child = world
            .get::<Children>(parent_entity)
            .is_some_and(|children| children.0.contains(&entity));
        if !parent_has_child {
            return Err(HierarchyError::InconsistentState(format!(
                "{entity} has Parent({parent_entity}), but the parent does not reference it"
            )));
        }

        let mut visited = HashSet::new();
        visited.insert(entity);
        let mut current = Some(parent_entity);
        while let Some(candidate) = current {
            if !visited.insert(candidate) {
                return Err(HierarchyError::CycleDetected {
                    child: entity,
                    parent: parent_entity,
                });
            }
            current = world.get::<Parent>(candidate).map(|p| p.0);
        }
    }

    for (parent, children) in world.iter::<Children>() {
        let mut unique = HashSet::new();
        for child in &children.0 {
            if !world.contains(*child) {
                return Err(WorldError::EntityNotFound(*child).into());
            }
            if !unique.insert(*child) {
                return Err(HierarchyError::InconsistentState(format!(
                    "{parent} contains duplicate child {child}"
                )));
            }
            if world.get::<Parent>(*child) != Some(&Parent(parent)) {
                return Err(HierarchyError::InconsistentState(format!(
                    "{parent} references {child}, but the child's Parent component disagrees"
                )));
            }
        }
    }
    Ok(())
}
