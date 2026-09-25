use extrem_ecs::World;
use extrem_engine::{Engine, EngineConfig, RenderExtractionStats, RenderExtractor, Stage};
use extrem_math::{Transform, Vec3};
use extrem_render::{RenderBackend, RenderCommand};
use extrem_scene::{Camera, GlobalTransform, Visibility};

#[path = "support/legacy_extraction.rs"]
mod legacy;
#[path = "support/extraction_fixture.rs"]
mod support;
use support::{Capture, Profile, fingerprint, fixture};

fn assert_parity(
    world: &World,
    aspect: f32,
    extractor: &mut RenderExtractor,
) -> RenderExtractionStats {
    let mut old = Capture::default();
    let mut new = Capture::default();
    legacy::legacy_submit(world, aspect, &mut old);
    let stats = extractor.submit(world, aspect, &mut new);
    assert_eq!(fingerprint(&new.commands), fingerprint(&old.commands));
    let mut fresh = Capture::default();
    RenderExtractor::default().submit(world, aspect, &mut fresh);
    assert_eq!(fingerprint(&fresh.commands), fingerprint(&old.commands));
    assert_eq!(
        new.commands.len(),
        stats.transform_commands + usize::from(stats.selected_camera.is_some())
    );
    assert_eq!(
        stats.camera_matrices,
        usize::from(stats.selected_camera.is_some())
    );
    stats
}

#[test]
fn four_profiles_preserve_every_command_bit_and_order() {
    let mut extractor = RenderExtractor::default();
    for profile in Profile::ALL {
        for nodes in [0, 1, 32, 512] {
            let (world, _) = fixture(profile, nodes).unwrap();
            let stats = assert_parity(&world, 16.0 / 9.0, &mut extractor);
            assert!(stats.transform_commands <= nodes, "{}", profile.name());
        }
    }
}

#[test]
fn all_256_component_presence_patterns_match_an_independent_oracle() {
    for code in 0..256usize {
        let mut world = World::new();
        let mut expected_transforms = Vec::new();
        let mut expected_camera = None;
        let mut rest = code;
        for index in 0..4 {
            let mode = rest % 4;
            rest /= 4;
            let id = world.spawn_empty();
            let local = Transform::from_translation(Vec3::new(index as f32, -0.0, 1.0));
            let global = Transform::from_translation(Vec3::new(index as f32 + 20.0, 2.0, 3.0));
            if mode & 1 != 0 {
                world.insert(id, local).unwrap();
            }
            if mode & 2 != 0 {
                world.insert(id, GlobalTransform(global)).unwrap();
            }
            world.insert(id, Camera::default()).unwrap();
            if mode != 0 {
                let transform = if mode & 2 != 0 { global } else { local };
                expected_camera.get_or_insert((id, transform));
                expected_transforms.push(RenderCommand::Transform {
                    entity: id,
                    translation: transform.translation,
                });
            }
        }
        let mut expected = Vec::new();
        if let Some((entity, transform)) = expected_camera {
            expected.push(RenderCommand::SetCamera {
                entity,
                view_projection: Camera::default().view_projection(transform, 1.0),
                world_position: transform.translation,
            });
        }
        expected.extend(expected_transforms);
        let mut actual = Capture::default();
        let mut extractor = RenderExtractor::default();
        extractor.submit(&world, 1.0, &mut actual);
        assert_eq!(
            fingerprint(&actual.commands),
            fingerprint(&expected),
            "{code}"
        );
        assert_parity(&world, 1.0, &mut extractor);
    }
}

#[test]
fn inactive_and_transformless_cameras_cannot_win_selection() {
    let (mut world, ids) = fixture(Profile::Local, 4).unwrap();
    world.get_mut::<Camera>(ids[0]).unwrap().active = false;
    world.remove::<Transform>(ids[1]).unwrap();
    world.insert(ids[3], Camera::default()).unwrap();
    world.insert(ids[2], Camera::default()).unwrap();
    let stats = assert_parity(&world, 1.0, &mut RenderExtractor::default());
    assert_eq!(stats.eligible_cameras, 2);
    assert_eq!(stats.selected_camera, Some(ids[2]));
}

#[test]
fn many_eligible_cameras_build_only_the_selected_matrix() {
    let (world, ids) = fixture(Profile::ManyCameras, 2048).unwrap();
    let stats = assert_parity(&world, 1.0, &mut RenderExtractor::default());
    assert_eq!(stats.eligible_cameras, 2048);
    assert_eq!(stats.selected_camera, Some(ids[0]));
    assert_eq!(stats.camera_matrices, 1);
}

#[test]
fn globals_take_precedence_and_visibility_contract_is_unchanged() {
    let mut world = World::new();
    let id = world.spawn(Transform::from_translation(Vec3::X));
    let global = Transform::from_translation(Vec3::new(99.0, -0.0, 0.0));
    world.insert(id, GlobalTransform(global)).unwrap();
    world.insert(id, Visibility(false)).unwrap();
    let mut capture = Capture::default();
    let stats = RenderExtractor::default().submit(&world, 1.0, &mut capture);
    assert_eq!(stats.transform_commands, 1);
    assert_eq!(
        capture.commands,
        vec![RenderCommand::Transform {
            entity: id,
            translation: global.translation,
        }]
    );
    assert_parity(&world, 1.0, &mut RenderExtractor::default());
}

#[test]
fn despawn_generation_reuse_and_component_removal_leave_no_old_commands() {
    let (mut world, ids) = fixture(Profile::Global, 4).unwrap();
    let mut extractor = RenderExtractor::default();
    assert_parity(&world, 1.0, &mut extractor);
    world.despawn(ids[0]).unwrap();
    let replacement = world.spawn(Transform::from_translation(Vec3::new(42.0, 0.0, 0.0)));
    world.insert(replacement, Camera::default()).unwrap();
    assert_ne!(replacement, ids[0]);
    world.remove::<GlobalTransform>(ids[1]).unwrap();
    world.remove::<Transform>(ids[2]).unwrap();
    let stats = assert_parity(&world, 1.0, &mut extractor);
    assert_eq!(stats.selected_camera, Some(replacement));
    assert_eq!(stats.transform_commands, 4);
}

#[test]
fn nonfinite_translation_bits_and_invalid_aspect_behavior_match_legacy() {
    let (mut world, ids) = fixture(Profile::Local, 4).unwrap();
    for id in &ids {
        world.remove::<Camera>(*id).unwrap();
    }
    for (id, value) in ids
        .iter()
        .zip([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0])
    {
        world.get_mut::<Transform>(*id).unwrap().translation.x = value;
    }
    let mut extractor = RenderExtractor::default();
    assert_parity(&world, 1.0, &mut extractor);
    world.insert(ids[3], Camera::default()).unwrap();
    for aspect in [f32::NAN, f32::INFINITY, -1.0, 0.0, 2.0] {
        assert_parity(&world, aspect, &mut extractor);
    }
}

#[test]
fn scratch_reuse_release_and_world_replacement_never_cache_results() {
    let (first, _) = fixture(Profile::Global, 4096).unwrap();
    let mut extractor = RenderExtractor::default();
    assert_parity(&first, 1.0, &mut extractor);
    let capacity = extractor.scratch_capacity();
    assert_parity(&first, 1.0, &mut extractor);
    assert_eq!(extractor.scratch_capacity(), capacity);
    let (mut second, ids) = fixture(Profile::Local, 2).unwrap();
    second.get_mut::<Transform>(ids[0]).unwrap().translation.x = 777.0;
    assert_parity(&second, 1.0, &mut extractor);
    assert_eq!(extractor.scratch_capacity(), capacity);
    let stats = assert_parity(&World::new(), 1.0, &mut extractor);
    assert_eq!(stats.transform_commands, 0);
    assert_eq!(stats.selected_camera, None);
    extractor.release_memory();
    assert_eq!(extractor.scratch_capacity(), 0);
    assert_parity(&second, 1.0, &mut extractor);
}

#[test]
fn actual_engine_submits_late_render_stage_edits_in_the_same_order() {
    let config = EngineConfig::default();
    let mut engine = Engine::with_renderer(Capture::default(), config);
    let (world, ids) = fixture(Profile::Global, 16).unwrap();
    engine.app_mut().world = world;
    let id = ids[3];
    engine
        .app_mut()
        .add_systems(Stage::Render, move |world, time| {
            world
                .get_mut::<GlobalTransform>(id)
                .unwrap()
                .0
                .translation
                .x = time.frame as f32;
        });
    for _ in 0..4 {
        engine.tick(0.0).unwrap();
        let mut reference = Capture::default();
        legacy::legacy_submit(engine.world(), config.viewport_aspect, &mut reference);
        assert_eq!(
            fingerprint(&engine.renderer().commands),
            fingerprint(&reference.commands)
        );
        assert_eq!(engine.last_extraction_stats().transform_commands, 16);
        assert_eq!(engine.last_frame_stats(), reference.end_frame());
    }
}

#[test]
fn engine_can_release_extraction_scratch_after_replacing_its_world() {
    let mut engine = Engine::new();
    engine.app_mut().world = fixture(Profile::Global, 4096).unwrap().0;
    engine.tick(0.0).unwrap();
    assert!(engine.last_extraction_stats().scratch_capacity >= 4096);
    engine.app_mut().world = World::new();
    engine.release_render_scratch();
    engine.tick(0.0).unwrap();
    assert_eq!(
        engine.last_extraction_stats(),
        RenderExtractionStats::default()
    );
    assert_eq!(engine.last_frame_stats().submitted_commands, 0);
}
