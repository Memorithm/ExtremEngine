use extrem_ecs::{Entity, World};
use extrem_math::{Quat, Transform, Vec3};
use extrem_scene::{
    Children, GlobalTransform, Parent, TransformPropagationStats, TransformPropagator, detach,
    propagate_transforms, set_parent, validate_hierarchy,
};

#[path = "support/propagation_fixture.rs"]
mod fixture;
#[path = "support/legacy_propagation.rs"]
mod legacy;
use fixture::{Shape, bits, fixture, local, snapshot};

fn restore(world: &mut World, initial: &[(Entity, GlobalTransform)]) {
    let ids: Vec<_> = world.iter::<GlobalTransform>().map(|(id, _)| id).collect();
    for id in ids {
        world.remove::<GlobalTransform>(id).unwrap();
    }
    for &(id, global) in initial {
        world.insert(id, global).unwrap();
    }
}

/// Execute all variants against the same World to preserve unspecified Transform-map order.
fn assert_parity(
    world: &mut World,
    scratch: &mut TransformPropagator,
) -> TransformPropagationStats {
    let initial: Vec<_> = world
        .iter::<GlobalTransform>()
        .map(|(id, g)| (id, *g))
        .collect();
    let locals: Vec<_> = world
        .iter::<Transform>()
        .map(|(id, t)| (id, bits(*t)))
        .collect();
    legacy::legacy_propagate_transforms(world);
    let expected = snapshot(world);
    restore(world, &initial);
    let stats = scratch.propagate(world);
    assert_eq!(stats.first_write_error, None);
    assert_eq!(snapshot(world), expected);
    restore(world, &initial);
    propagate_transforms(world);
    assert_eq!(snapshot(world), expected);
    let after: Vec<_> = world
        .iter::<Transform>()
        .map(|(id, t)| (id, bits(*t)))
        .collect();
    assert_eq!(locals, after);
    stats
}

#[test]
fn all_shapes_match_legacy_and_independent_parent_first_oracle() {
    let mut scratch = TransformPropagator::default();
    for shape in Shape::ALL {
        for nodes in [0, 1, 32, 257] {
            let (mut world, entities) = fixture(shape, nodes).unwrap();
            validate_hierarchy(&world).unwrap();
            let stats = assert_parity(&mut world, &mut scratch);
            assert_eq!(stats.visited, nodes, "{}", shape.name());
            assert_eq!(stats.skipped_revisits, 0);
            // This oracle walks known parent indices in ascending order, not the DFS stack.
            let mut expected = Vec::new();
            for (index, entity) in entities.into_iter().enumerate() {
                let global = match shape.parent(index) {
                    Some(parent) => Transform::combine(expected[parent], local(index)),
                    None => local(index),
                };
                expected.push(global);
                assert_eq!(
                    bits(world.get::<GlobalTransform>(entity).unwrap().0),
                    bits(global)
                );
            }
        }
    }
}

#[test]
fn all_625_four_node_parent_graphs_preserve_legacy_reachability() {
    let mut scratch = TransformPropagator::default();
    for code in 0..625usize {
        let (mut world, ids) = fixture(Shape::Independent, 4).unwrap();
        let mut rest = code;
        let mut children = vec![Vec::new(); 4];
        for id in &ids {
            let parent = rest % 5;
            rest /= 5;
            if parent < 4 {
                world.insert(*id, Parent(ids[parent])).unwrap();
                children[parent].push(*id);
            }
        }
        for (id, children) in ids.into_iter().zip(children) {
            world.insert(id, Children(children)).unwrap();
        }
        let stats = assert_parity(&mut world, &mut scratch);
        assert!(stats.visited <= 4);
    }
}

#[test]
fn deep_and_wide_twenty_thousand_nodes_are_iterative_and_reusable() {
    let mut scratch = TransformPropagator::default();
    for shape in [Shape::Chain, Shape::Wide] {
        let (mut world, _) = fixture(shape, 20_000).unwrap();
        let stats = assert_parity(&mut world, &mut scratch);
        assert_eq!(stats.visited, 20_000);
        assert_eq!(stats.child_links, 19_999);
        let capacity = scratch.scratch_capacity();
        assert_eq!(scratch.propagate(&mut world).visited, 20_000);
        assert_eq!(scratch.scratch_capacity(), capacity);
    }
}

#[test]
fn reachable_corrupt_cycles_duplicates_and_shared_child_terminate() {
    let (mut world, ids) = fixture(Shape::Wide, 3).unwrap();
    world
        .insert(ids[0], Children(vec![ids[1], ids[2], ids[1]]))
        .unwrap();
    world
        .insert(ids[1], Children(vec![ids[0], ids[2]]))
        .unwrap();
    let mut scratch = TransformPropagator::default();
    let stats = assert_parity(&mut world, &mut scratch);
    assert_eq!(stats.visited, 3);
    assert_eq!(stats.child_links, 5);
    assert_eq!(stats.skipped_revisits, 3);
}

#[test]
fn missing_locals_and_stale_or_extreme_child_ids_are_not_followed() {
    let (mut world, ids) = fixture(Shape::Chain, 4).unwrap();
    world.remove::<Transform>(ids[1]).unwrap();
    world.despawn(ids[3]).unwrap();
    let replacement = world.spawn(Transform::IDENTITY);
    let sentinel = GlobalTransform(Transform::from_translation(Vec3::new(123.0, 0.0, 0.0)));
    world.insert(ids[2], sentinel).unwrap();
    world
        .insert(
            ids[0],
            Children(vec![
                ids[1],
                ids[3],
                Entity::from_raw_parts(u32::MAX, u32::MAX),
            ]),
        )
        .unwrap();
    let mut scratch = TransformPropagator::default();
    let stats = assert_parity(&mut world, &mut scratch);
    assert_eq!(stats.missing_child_transforms, 3);
    assert_eq!(world.get::<GlobalTransform>(ids[2]), Some(&sentinel));
    assert_eq!(
        world.get::<GlobalTransform>(replacement),
        Some(&GlobalTransform(Transform::IDENTITY))
    );
}

#[test]
fn absent_globals_are_inserted_and_unreachable_globals_are_preserved() {
    let (mut world, ids) = fixture(Shape::Wide, 4).unwrap();
    for id in &ids {
        world.remove::<GlobalTransform>(*id).unwrap();
    }
    let orphan = world.spawn(Transform::IDENTITY);
    world
        .insert(orphan, Parent(Entity::from_raw(u32::MAX)))
        .unwrap();
    let mut scratch = TransformPropagator::default();
    let stats = assert_parity(&mut world, &mut scratch);
    assert_eq!(stats.inserted_globals, 4);
    assert_eq!(stats.visited, 4);
    assert!(world.get::<GlobalTransform>(orphan).is_none());
    assert_eq!(scratch.propagate(&mut world).inserted_globals, 0);
}

#[test]
fn local_edits_reparent_detach_and_slot_reuse_rebuild_every_call() {
    let (mut world, ids) = fixture(Shape::Wide, 4).unwrap();
    let mut scratch = TransformPropagator::default();
    assert_parity(&mut world, &mut scratch);
    world.get_mut::<Transform>(ids[0]).unwrap().translation = Vec3::new(9.0, 2.0, -3.0);
    assert_parity(&mut world, &mut scratch);
    set_parent(&mut world, ids[3], ids[1]).unwrap();
    assert_parity(&mut world, &mut scratch);
    detach(&mut world, ids[3]).unwrap();
    assert_eq!(assert_parity(&mut world, &mut scratch).roots, 2);
    world.despawn(ids[3]).unwrap();
    let replacement = world.spawn(Transform::from_translation(Vec3::new(-99.0, 0.0, 0.0)));
    assert_ne!(replacement, ids[3]);
    assert_parity(&mut world, &mut scratch);
    assert_eq!(
        world
            .get::<GlobalTransform>(replacement)
            .unwrap()
            .0
            .translation
            .x,
        -99.0
    );
    validate_hierarchy(&world).unwrap();
}

#[test]
fn world_replacement_empty_world_and_release_do_not_leak_visited_state() {
    let mut scratch = TransformPropagator::default();
    let (mut first, _) = fixture(Shape::Wide, 1024).unwrap();
    assert_parity(&mut first, &mut scratch);
    let retained = scratch.scratch_capacity();
    let (mut second, ids) = fixture(Shape::Chain, 2).unwrap();
    second.get_mut::<Transform>(ids[0]).unwrap().translation.x = 44.0;
    assert_parity(&mut second, &mut scratch);
    assert_eq!(scratch.scratch_capacity(), retained);
    let stats = scratch.propagate(&mut World::new());
    assert_eq!((stats.roots, stats.visited, stats.child_links), (0, 0, 0));
    scratch.release_memory();
    assert_eq!(scratch.scratch_capacity(), (0, 0));
    assert_parity(&mut second, &mut scratch);
}

#[test]
fn rotations_nonuniform_scales_and_signed_zero_preserve_exact_arithmetic() {
    let (mut world, ids) = fixture(Shape::Chain, 12).unwrap();
    for (index, id) in ids.iter().enumerate() {
        world
            .insert(
                *id,
                Transform {
                    translation: Vec3::new(index as f32 * 0.25, -0.0, -0.125),
                    rotation: Quat::from_axis_angle(Vec3::X, index as f32 * 0.03),
                    scale: Vec3::new(1.5, -0.5, 0.75),
                },
            )
            .unwrap();
    }
    assert_parity(&mut world, &mut TransformPropagator::default());
}

#[test]
fn existing_nonfinite_behavior_is_not_silently_sanitized() {
    let (mut world, ids) = fixture(Shape::Independent, 4).unwrap();
    for (id, value) in ids
        .into_iter()
        .zip([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0])
    {
        world.get_mut::<Transform>(id).unwrap().translation.x = value;
    }
    assert_parity(&mut world, &mut TransformPropagator::default());
}
