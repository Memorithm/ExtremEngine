use extrem_ecs::World;
use extrem_editor::{EditorCommand, EditorState};
use extrem_math::{Transform, Vec3};
use extrem_scene::{Children, Scene, Visibility, validate_hierarchy};

#[test]
fn failed_undo_preserves_the_record_for_retry() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::new(2.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let removed = world.remove::<Transform>(entity).unwrap().unwrap();
    assert!(editor.undo(&mut world).is_err());
    world.insert(entity, removed).unwrap();
    assert!(editor.undo(&mut world).unwrap());
    assert_eq!(
        world.get::<Transform>(entity).unwrap().translation,
        Vec3::ZERO
    );
}

#[test]
fn failed_redo_preserves_the_record_for_retry() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::new(2.0, 0.0, 0.0),
            },
        )
        .unwrap();
    assert!(editor.undo(&mut world).unwrap());
    let removed = world.remove::<Transform>(entity).unwrap().unwrap();
    assert!(editor.redo(&mut world).is_err());
    world.insert(entity, removed).unwrap();
    assert!(editor.redo(&mut world).unwrap());
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 2.0);
}

#[test]
fn non_finite_translation_is_rejected_without_mutation() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let mut editor = EditorState::default();
        assert!(
            editor
                .apply(
                    &mut world,
                    EditorCommand::Translate {
                        entity,
                        delta: Vec3::new(value, 0.0, 0.0),
                    },
                )
                .is_err()
        );
        assert_eq!(world.get::<Transform>(entity), Some(&Transform::default()));
        assert!(!editor.undo(&mut world).unwrap());
    }
}

#[test]
fn finite_translation_overflow_is_rejected_without_mutation() {
    let mut world = World::new();
    let initial = Transform::from_translation(Vec3::new(f32::MAX, 0.0, 0.0));
    let entity = world.spawn(initial);
    let mut editor = EditorState::default();
    assert!(
        editor
            .apply(
                &mut world,
                EditorCommand::Translate {
                    entity,
                    delta: Vec3::new(f32::MAX, 0.0, 0.0),
                },
            )
            .is_err()
    );
    assert_eq!(world.get::<Transform>(entity), Some(&initial));
}

#[test]
fn undo_restores_exact_translation_despite_float_rounding() {
    let mut world = World::new();
    let initial = Transform::from_translation(Vec3::new(16_777_216.0, -0.0, 0.0));
    let entity = world.spawn(initial);
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::new(1.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let applied = *world.get::<Transform>(entity).unwrap();
    assert!(editor.undo(&mut world).unwrap());
    let restored = world.get::<Transform>(entity).unwrap().translation;
    assert_eq!(restored.x.to_bits(), initial.translation.x.to_bits());
    assert_eq!(restored.y.to_bits(), initial.translation.y.to_bits());
    assert!(editor.redo(&mut world).unwrap());
    assert_eq!(world.get::<Transform>(entity), Some(&applied));
}

#[test]
fn undo_restores_absence_of_visibility_component() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::SetVisible {
                entity,
                visible: false,
            },
        )
        .unwrap();
    assert!(editor.undo(&mut world).unwrap());
    assert_eq!(world.get::<Visibility>(entity), None);
    assert!(editor.redo(&mut world).unwrap());
    assert_eq!(world.get::<Visibility>(entity), Some(&Visibility(false)));
}

#[test]
fn deleting_a_subtree_preserves_surviving_hierarchy() {
    let mut world = World::new();
    let mut scene = Scene::new("editor deletion");
    let root = scene
        .spawn_entity(&mut world, "root", Transform::default())
        .unwrap();
    let child = scene
        .spawn_child(&mut world, root, "child", Transform::default())
        .unwrap();
    let grandchild = scene
        .spawn_child(&mut world, child, "grandchild", Transform::default())
        .unwrap();
    let mut editor = EditorState::default();
    editor
        .apply(&mut world, EditorCommand::Select(grandchild))
        .unwrap();
    editor
        .apply(&mut world, EditorCommand::Delete(child))
        .unwrap();
    assert!(world.contains(root));
    assert!(!world.contains(child));
    assert!(!world.contains(grandchild));
    assert_eq!(editor.selection, None);
    assert!(world.get::<Children>(root).unwrap().0.is_empty());
    validate_hierarchy(&world).unwrap();
}

#[test]
fn failed_delete_does_not_change_selection() {
    let mut world = World::new();
    let stale = world.spawn(Transform::default());
    world.despawn(stale).unwrap();
    let mut editor = EditorState::default();
    editor.selection = Some(stale);
    assert!(
        editor
            .apply(&mut world, EditorCommand::Delete(stale))
            .is_err()
    );
    assert_eq!(editor.selection, Some(stale));
}

#[test]
fn irreversible_delete_is_a_history_barrier() {
    let mut world = World::new();
    let edited = world.spawn(Transform::default());
    let deleted = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity: edited,
                delta: Vec3::new(1.0, 0.0, 0.0),
            },
        )
        .unwrap();
    assert!(editor.undo(&mut world).unwrap());
    editor
        .apply(&mut world, EditorCommand::Delete(deleted))
        .unwrap();
    assert!(!editor.redo(&mut world).unwrap());
    assert!(!editor.undo(&mut world).unwrap());
}

#[test]
fn corrupt_hierarchy_is_rejected_before_delete() {
    let mut world = World::new();
    let parent = world.spawn(Transform::default());
    let unrelated = world.spawn(Transform::default());
    // A one-sided edge must not authorize deletion of an unrelated entity.
    world.insert(parent, Children(vec![unrelated])).unwrap();
    let mut editor = EditorState::default();
    editor
        .apply(&mut world, EditorCommand::Select(parent))
        .unwrap();
    assert!(
        editor
            .apply(&mut world, EditorCommand::Delete(parent))
            .is_err()
    );
    assert!(world.contains(parent));
    assert!(world.contains(unrelated));
    assert_eq!(editor.selection, Some(parent));
    assert_eq!(world.get::<Children>(parent).unwrap().0, vec![unrelated]);
}
