use extrem_ecs::World;
use extrem_editor::{EditorCommand, EditorError, EditorLimits, EditorState};
use extrem_math::{Transform, Vec3};
use extrem_scene::{Children, Name, Visibility};

#[test]
fn history_is_bounded_across_edits_undo_and_redo() {
    for limit in [0, 1, 3, 16] {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let mut editor = EditorState::with_limits(EditorLimits {
            max_history_entries: limit,
            ..EditorLimits::default()
        });
        for step in 0..32 {
            editor
                .apply(
                    &mut world,
                    EditorCommand::Translate {
                        entity,
                        delta: Vec3::X,
                    },
                )
                .unwrap();
            assert_eq!(editor.undo_len(), (step + 1).min(limit));
            assert_eq!(editor.redo_len(), 0);
        }
        for undone in 0..limit {
            assert!(editor.undo(&mut world).unwrap());
            assert_eq!(editor.undo_len() + editor.redo_len(), limit);
            assert_eq!(editor.redo_len(), undone + 1);
        }
        assert!(!editor.undo(&mut world).unwrap());
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation.x,
            (32 - limit) as f32
        );
        for _ in 0..limit {
            assert!(editor.redo(&mut world).unwrap());
            assert_eq!(editor.undo_len() + editor.redo_len(), limit);
        }
        assert!(!editor.redo(&mut world).unwrap());
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 32.0);
    }
}

#[test]
fn failed_edit_and_selection_preserve_redo_but_successful_edit_clears_it() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::X,
            },
        )
        .unwrap();
    assert!(editor.undo(&mut world).unwrap());
    assert!(
        editor
            .apply(
                &mut world,
                EditorCommand::SetTranslation {
                    entity,
                    translation: Vec3::new(0.0, f32::INFINITY, 0.0),
                },
            )
            .is_err()
    );
    assert_eq!((editor.undo_len(), editor.redo_len()), (0, 1));
    editor
        .apply(&mut world, EditorCommand::Select(entity))
        .unwrap();
    assert_eq!((editor.undo_len(), editor.redo_len()), (0, 1));
    assert!(editor.redo(&mut world).unwrap());
    assert!(editor.undo(&mut world).unwrap());
    editor
        .apply(
            &mut world,
            EditorCommand::SetVisible {
                entity,
                visible: false,
            },
        )
        .unwrap();
    assert_eq!((editor.undo_len(), editor.redo_len()), (1, 0));
}

#[test]
fn names_are_bounded_in_utf8_bytes_before_any_mutation() {
    let mut world = World::new();
    let entity = world.spawn(Name::from("ok"));
    let mut editor = EditorState::with_limits(EditorLimits {
        max_name_bytes: 4,
        ..EditorLimits::default()
    });
    editor
        .apply(
            &mut world,
            EditorCommand::Rename {
                entity,
                name: "éé".into(),
            },
        )
        .unwrap();
    assert!(matches!(
        editor.apply(
            &mut world,
            EditorCommand::Rename {
                entity,
                name: "ééé".into(),
            },
        ),
        Err(EditorError::NameTooLong { max_bytes: 4 })
    ));
    assert_eq!(world.get::<Name>(entity).unwrap().0, "éé");
    assert_eq!(editor.undo_len(), 1);
    assert!(editor.undo(&mut world).unwrap());
    assert_eq!(world.get::<Name>(entity).unwrap().0, "ok");
    // An oversized old value must not be copied into an inverse record either.
    world.insert(entity, Name::from("oversized")).unwrap();
    assert!(matches!(
        editor.apply(
            &mut world,
            EditorCommand::Rename {
                entity,
                name: "new".into(),
            },
        ),
        Err(EditorError::NameTooLong { .. })
    ));
    assert_eq!(world.get::<Name>(entity).unwrap().0, "oversized");
    assert_eq!((editor.undo_len(), editor.redo_len()), (0, 1));
}

#[test]
fn mixed_commands_restore_complete_inspector_snapshots_in_order() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    world.insert(entity, Name::from("before")).unwrap();
    let mut editor = EditorState::default();
    let before = editor.inspect(&world, entity).unwrap();
    let commands = [
        EditorCommand::Rename {
            entity,
            name: "after".into(),
        },
        EditorCommand::SetTranslation {
            entity,
            translation: Vec3::new(10.0, -2.0, 3.0),
        },
        EditorCommand::SetVisible {
            entity,
            visible: false,
        },
        EditorCommand::RemoveVisibility(entity),
    ];
    for command in commands {
        editor.apply(&mut world, command).unwrap();
    }
    let after = editor.inspect(&world, entity).unwrap();
    for _ in 0..4 {
        assert!(editor.undo(&mut world).unwrap());
    }
    assert_eq!(editor.inspect(&world, entity).unwrap(), before);
    for _ in 0..4 {
        assert!(editor.redo(&mut world).unwrap());
    }
    assert_eq!(editor.inspect(&world, entity).unwrap(), after);
}

#[test]
fn stale_history_cannot_mutate_a_reused_entity_slot() {
    let mut world = World::new();
    let original = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Translate {
                entity: original,
                delta: Vec3::X,
            },
        )
        .unwrap();
    world.despawn(original).unwrap();
    let replacement = world.spawn(Transform::from_translation(Vec3::new(7.0, 0.0, 0.0)));
    assert_ne!(original, replacement);
    for _ in 0..2 {
        assert!(editor.undo(&mut world).is_err());
        assert_eq!((editor.undo_len(), editor.redo_len()), (1, 0));
        assert_eq!(
            world.get::<Transform>(replacement).unwrap().translation.x,
            7.0
        );
    }
    editor.clear_history();
    assert!(!editor.undo(&mut world).unwrap());
}

#[test]
fn failed_rename_replay_can_be_retried_after_component_repair() {
    let mut world = World::new();
    let entity = world.spawn(Name::from("before"));
    let mut editor = EditorState::default();
    editor
        .apply(
            &mut world,
            EditorCommand::Rename {
                entity,
                name: "after".into(),
            },
        )
        .unwrap();
    let removed = world.remove::<Name>(entity).unwrap().unwrap();
    assert!(matches!(
        editor.undo(&mut world),
        Err(EditorError::MissingName(missing)) if missing == entity
    ));
    assert_eq!((editor.undo_len(), editor.redo_len()), (1, 0));
    assert!(world.get::<Name>(entity).is_none());
    world.insert(entity, removed).unwrap();
    assert!(editor.undo(&mut world).unwrap());
    assert_eq!(world.get::<Name>(entity).unwrap().0, "before");
}

#[test]
fn failed_delete_preserves_both_history_stacks() {
    let mut world = World::new();
    let entity = world.spawn(Transform::default());
    let mut editor = EditorState::default();
    for _ in 0..2 {
        editor
            .apply(
                &mut world,
                EditorCommand::Translate {
                    entity,
                    delta: Vec3::X,
                },
            )
            .unwrap();
    }
    assert!(editor.undo(&mut world).unwrap());
    world.insert(entity, Children(vec![entity])).unwrap();
    assert!(
        editor
            .apply(&mut world, EditorCommand::Delete(entity))
            .is_err()
    );
    assert_eq!((editor.undo_len(), editor.redo_len()), (1, 1));
    world.remove::<Children>(entity).unwrap();
    editor
        .apply(&mut world, EditorCommand::Delete(entity))
        .unwrap();
    assert_eq!((editor.undo_len(), editor.redo_len()), (0, 0));
}

#[test]
fn non_finite_existing_translation_is_rejected_before_recording() {
    let mut world = World::new();
    let entity = world.spawn(Transform::from_translation(Vec3::new(f32::NAN, 0.0, 0.0)));
    let mut editor = EditorState::default();
    let before_bits = world
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .x
        .to_bits();
    assert!(matches!(
        editor.apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::X,
            },
        ),
        Err(EditorError::NonFiniteTranslation(invalid)) if invalid == entity
    ));
    assert_eq!(
        world
            .get::<Transform>(entity)
            .unwrap()
            .translation
            .x
            .to_bits(),
        before_bits
    );
    assert_eq!((editor.undo_len(), editor.redo_len()), (0, 0));
}

#[test]
fn removing_visibility_preserves_an_explicit_false_value_on_undo() {
    let mut world = World::new();
    let entity = world.spawn(Visibility(false));
    let mut editor = EditorState::default();
    editor
        .apply(&mut world, EditorCommand::RemoveVisibility(entity))
        .unwrap();
    assert!(world.get::<Visibility>(entity).is_none());
    assert!(editor.undo(&mut world).unwrap());
    assert_eq!(world.get::<Visibility>(entity), Some(&Visibility(false)));
    assert!(editor.redo(&mut world).unwrap());
    assert!(world.get::<Visibility>(entity).is_none());
}
