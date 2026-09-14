use extrem_ecs::World;
use extrem_engine::{Engine, GlobalTransform, Stage, TransformPropagationStats};
use extrem_math::{Transform, Vec3};
use extrem_scene::set_parent;

#[test]
fn runtime_reuses_scratch_and_propagates_update_before_render() {
    let mut engine = Engine::new();
    let root = engine.world_mut().spawn(Transform::IDENTITY);
    let child = engine.world_mut().spawn(Transform::from_translation(Vec3::X));
    set_parent(engine.world_mut(), child, root).unwrap();
    engine.app_mut().add_systems(Stage::Update, move |world, _| {
        world.get_mut::<Transform>(root).unwrap().translation.x += 1.0;
    });
    engine.app_mut().add_systems(Stage::Render, move |world, time| {
        let global = world.get::<GlobalTransform>(child).unwrap().0;
        assert_eq!(global.translation.x, time.frame as f32 + 1.0);
        let stats = world.get_resource::<TransformPropagationStats>().unwrap();
        assert_eq!((stats.roots, stats.visited, stats.child_links), (1, 2, 1));
        assert_eq!(stats.first_write_error, None);
    });
    engine.tick(0.0);
    let first = *engine.world().get_resource::<TransformPropagationStats>().unwrap();
    assert_eq!(first.inserted_globals, 2);
    for _ in 0..8 {
        engine.tick(0.0);
        let stats = engine.world().get_resource::<TransformPropagationStats>().unwrap();
        assert_eq!(stats.inserted_globals, 0);
        assert_eq!(stats.pending_capacity, first.pending_capacity);
        assert_eq!(stats.visited_capacity, first.visited_capacity);
        assert_eq!(engine.last_frame_stats().submitted_commands, 2);
    }
}

#[test]
fn replacing_app_world_does_not_reuse_old_globals_or_entity_visits() {
    let mut engine = Engine::new();
    let old = engine.world_mut().spawn(Transform::from_translation(Vec3::X));
    engine.tick(0.0);
    engine.app_mut().world = World::new();
    let new = engine.world_mut().spawn(Transform::from_translation(Vec3::new(33.0, 0.0, 0.0)));
    assert_eq!(old, new); // IDs are world-local; scratch must never treat them as cached results.
    engine.tick(0.0);
    assert_eq!(engine.world().get::<GlobalTransform>(new).unwrap().0.translation.x, 33.0);
    let stats = engine.world().get_resource::<TransformPropagationStats>().unwrap();
    assert_eq!((stats.roots, stats.visited, stats.inserted_globals), (1, 1, 1));
    assert_eq!(stats.first_write_error, None);
}
