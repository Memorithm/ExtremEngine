use extrem_ecs::{Entity, World};
use extrem_scene::{
    Children, HierarchyValidationStats, HierarchyValidator, Parent, validate_hierarchy,
};

#[path = "support/legacy_hierarchy.rs"]
mod legacy;

fn from_parents(parents: &[Option<usize>]) -> (World, Vec<Entity>) {
    let mut world = World::new();
    let entities: Vec<_> = parents.iter().map(|_| world.spawn_empty()).collect();
    let mut children = vec![Vec::new(); parents.len()];
    for (child, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            world
                .insert(entities[child], Parent(entities[*parent]))
                .unwrap();
            children[*parent].push(entities[child]);
        }
    }
    for (entity, children) in entities.iter().zip(children) {
        world.insert(*entity, Children(children)).unwrap();
    }
    (world, entities)
}

// Independent leaf-removal oracle over indices, not DFS or ECS lookups.
fn acyclic_by_leaf_removal(parents: &[Option<usize>]) -> bool {
    let mut child_count = vec![0; parents.len()];
    for parent in parents.iter().flatten() {
        child_count[*parent] += 1;
    }
    let mut leaves: Vec<_> = child_count
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    let mut removed = 0;
    while let Some(leaf) = leaves.pop() {
        removed += 1;
        if let Some(parent) = parents[leaf] {
            child_count[parent] -= 1;
            if child_count[parent] == 0 {
                leaves.push(parent);
            }
        }
    }
    removed == parents.len()
}

#[test]
fn exhaustive_four_node_parent_graphs_match_two_independent_references() {
    let mut validator = HierarchyValidator::default();
    // Each node has no parent or one of the four nodes: 5^4 = 625 graphs.
    for code in 0..625 {
        let mut value = code;
        let parents: Vec<_> = (0..4)
            .map(|_| {
                let digit = value % 5;
                value /= 5;
                (digit != 4).then_some(digit)
            })
            .collect();
        let (world, _) = from_parents(&parents);
        let expected = acyclic_by_leaf_removal(&parents);
        assert_eq!(legacy::legacy_validate_hierarchy(&world).is_ok(), expected);
        assert_eq!(validator.validate(&world).is_ok(), expected, "graph {code}");
        assert_eq!(validate_hierarchy(&world).is_ok(), expected);
    }
}

#[test]
fn deep_chain_enters_each_parent_link_once_without_recursion() {
    let parents: Vec<_> = (0..20_000).map(|i: usize| i.checked_sub(1)).collect();
    let (world, _) = from_parents(&parents);
    let stats = HierarchyValidator::default().validate(&world).unwrap();
    assert_eq!(stats.parent_links, 19_999);
    assert_eq!(stats.child_links, stats.parent_links);
    assert_eq!(stats.ancestor_steps, stats.parent_links);
}

#[test]
fn wide_tree_checks_each_child_link_once() {
    let mut parents = vec![Some(0); 20_000];
    parents[0] = None;
    let (world, _) = from_parents(&parents);
    let stats = HierarchyValidator::default().validate(&world).unwrap();
    assert_eq!(stats.parent_links, 19_999);
    assert_eq!(stats.child_links, stats.parent_links);
    assert_eq!(stats.ancestor_steps, stats.parent_links);
}

#[test]
fn empty_world_and_isolated_roots_are_valid() {
    let mut validator = HierarchyValidator::default();
    let (world, _) = from_parents(&[None; 8]);
    assert_eq!(
        validator.validate(&world).unwrap(),
        HierarchyValidationStats::default()
    );
    assert_eq!(
        validator.validate(&World::new()).unwrap(),
        HierarchyValidationStats::default()
    );
}

#[test]
fn missing_reverse_edge_is_rejected() {
    let (mut world, entities) = from_parents(&[None, Some(0)]);
    world.remove::<Children>(entities[0]).unwrap();
    assert!(validate_hierarchy(&world).is_err());
    assert!(legacy::legacy_validate_hierarchy(&world).is_err());
}

#[test]
fn one_sided_child_edge_is_rejected() {
    let (mut world, entities) = from_parents(&[None, Some(0)]);
    world.remove::<Parent>(entities[1]).unwrap();
    assert!(validate_hierarchy(&world).is_err());
}

#[test]
fn duplicate_children_are_rejected_without_mutation() {
    let (mut world, entities) = from_parents(&[None, Some(0)]);
    let duplicate = Children(vec![entities[1], entities[1]]);
    world.insert(entities[0], duplicate.clone()).unwrap();
    assert!(validate_hierarchy(&world).is_err());
    assert_eq!(world.get::<Children>(entities[0]), Some(&duplicate));
    assert_eq!(world.entity_count(), 2);
}

#[test]
fn inconsistent_parent_is_rejected() {
    let (mut world, entities) = from_parents(&[None, Some(0), None]);
    world.insert(entities[1], Parent(entities[2])).unwrap();
    assert!(validate_hierarchy(&world).is_err());
}

#[test]
fn stale_parent_generation_cannot_be_replaced_by_live_slot() {
    let (mut world, entities) = from_parents(&[None, Some(0)]);
    world.despawn(entities[0]).unwrap();
    let replacement = world.spawn_empty();
    assert_eq!(replacement.index(), entities[0].index());
    assert_ne!(replacement, entities[0]);
    world
        .insert(replacement, Children(vec![entities[1]]))
        .unwrap();
    assert!(validate_hierarchy(&world).is_err());
}

#[test]
fn stale_child_and_extreme_raw_ids_are_rejected() {
    let (mut world, entities) = from_parents(&[None, Some(0)]);
    world.despawn(entities[1]).unwrap();
    assert!(validate_hierarchy(&world).is_err());
    world
        .insert(
            entities[0],
            Children(vec![Entity::from_raw_parts(u32::MAX, u32::MAX)]),
        )
        .unwrap();
    assert!(validate_hierarchy(&world).is_err());
    world.insert(entities[0], Children::default()).unwrap();
    world
        .insert(entities[0], Parent(Entity::from_raw(u32::MAX)))
        .unwrap();
    assert!(validate_hierarchy(&world).is_err());
}

#[test]
fn reuse_revalidates_after_error_repair_and_replacement_world() {
    let mut validator = HierarchyValidator::default();
    let (mut world, entities) = from_parents(&[Some(1), Some(0)]);
    assert!(validator.validate(&world).is_err());
    world.remove::<Parent>(entities[0]).unwrap();
    world.insert(entities[1], Children::default()).unwrap();
    assert_eq!(validator.validate(&world).unwrap().parent_links, 1);
    // Identical entity bits in a different world must not reuse a previous result.
    let (other, _) = from_parents(&[Some(1), Some(0)]);
    assert!(validator.validate(&other).is_err());
    validator.release_memory();
    assert_eq!(validator.validate(&world).unwrap().ancestor_steps, 1);
}
