use extrem_ecs::World;
use extrem_engine::{Engine, MeshData, MeshError, MeshExtractor, MeshInstance, MeshVertex, Stage};
use extrem_math::{Transform, Vec3};
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand};
use extrem_scene::{GlobalTransform, Visibility};

fn instance() -> MeshInstance {
    MeshInstance {
        geometry: MeshData::new(
            vec![MeshVertex {
                position: [0.0; 3],
                color: [1.0; 3],
            }],
            vec![0, 0, 0],
        )
        .unwrap(),
        color: [1.0; 4],
    }
}

#[test]
fn visible_meshes_use_global_precedence_and_deterministic_entity_order() {
    let mut world = World::new();
    let a = world.spawn(Transform::from_translation(Vec3::X));
    let b = world.spawn(Transform::from_translation(Vec3::Y));
    world.insert(b, instance()).unwrap();
    world.insert(a, instance()).unwrap();
    world
        .insert(
            a,
            GlobalTransform(Transform::from_translation(Vec3::new(3.0, 0.0, 0.0))),
        )
        .unwrap();
    let mut extractor = MeshExtractor::default();
    assert_eq!(extractor.extract(&world).unwrap().visible, 2);
    assert_eq!(extractor.draws()[0].model[12], 3.0);
    assert_eq!(extractor.draws()[1].model[13], 1.0);
    world.insert(a, Visibility(false)).unwrap();
    let stats = extractor.extract(&world).unwrap();
    assert_eq!((stats.visible, stats.hidden), (1, 1));
    world.remove::<Transform>(b).unwrap();
    assert_eq!(extractor.extract(&world).unwrap().missing_transform, 1);
    assert!(extractor.draws().is_empty());
}

#[test]
fn invalid_input_does_not_leave_a_partial_or_previous_draw_slice() {
    let mut world = World::new();
    let a = world.spawn(Transform::IDENTITY);
    world.insert(a, instance()).unwrap();
    let mut extractor = MeshExtractor::default();
    assert_eq!(extractor.extract(&world).unwrap().visible, 1);
    world.get_mut::<MeshInstance>(a).unwrap().color[3] = 0.5;
    assert_eq!(extractor.extract(&world), Err(MeshError::InvalidColor));
    assert!(extractor.draws().is_empty());
    world.get_mut::<MeshInstance>(a).unwrap().color[3] = 1.0;
    world.get_mut::<Transform>(a).unwrap().translation.x = f32::NAN;
    assert_eq!(extractor.extract(&world), Err(MeshError::InvalidMatrix));
    assert!(extractor.draws().is_empty());
}

#[test]
fn removed_recycled_and_replaced_world_entities_do_not_leak_draws() {
    let mut world = World::new();
    let a = world.spawn(Transform::IDENTITY);
    world.insert(a, instance()).unwrap();
    let mut extractor = MeshExtractor::default();
    extractor.extract(&world).unwrap();
    world.despawn(a).unwrap();
    world.spawn(Transform::IDENTITY);
    assert_eq!(extractor.extract(&world).unwrap().visible, 0);
    assert_eq!(extractor.extract(&World::new()).unwrap().visible, 0);
    assert!(extractor.draws().is_empty());
}

#[derive(Default)]
struct Probe {
    open: bool,
    extracts: usize,
}
impl RenderBackend for Probe {
    fn begin_frame(&mut self, _: FrameInfo) {
        self.open = true;
    }
    fn submit(&mut self, _: RenderCommand) {
        assert!(self.open);
    }
    fn end_frame(&mut self) -> FrameStats {
        self.open = false;
        FrameStats::default()
    }
}

#[test]
fn actual_engine_hook_runs_after_stages_and_never_on_rejected_graph() {
    let mut engine = Engine::with_renderer(Probe::default(), Default::default());
    engine.app_mut().add_systems(Stage::Render, |world, _| {
        world.insert_resource(42_u32);
    });
    engine.set_backend_extractor(|world, probe| {
        assert!(probe.open);
        assert_eq!(world.get_resource::<u32>(), Some(&42));
        probe.extracts += 1;
    });
    let bad = engine.render_graph_mut().add_pass("bad");
    engine.render_graph_mut().add_dependency(bad, bad).unwrap();
    assert!(engine.tick(0.0).is_err());
    assert_eq!(engine.renderer().extracts, 0);
    engine
        .render_graph_mut()
        .remove_dependency(bad, bad)
        .unwrap();
    engine.tick(0.0).unwrap();
    assert_eq!(engine.renderer().extracts, 1);
    assert!(!engine.renderer().open);
}

#[test]
fn viewport_updates_reject_nonfinite_or_nonpositive_aspect() {
    let mut engine = Engine::new();
    assert!(engine.set_viewport_aspect(2.0));
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!engine.set_viewport_aspect(value));
        assert_eq!(engine.config().viewport_aspect, 2.0);
    }
}

#[test]
fn visible_draw_limit_rejects_without_retaining_partial_output() {
    let mut world = World::new();
    let component = instance();
    for _ in 0..=extrem_gpu::MAX_FRAME_DRAWS {
        let entity = world.spawn(Transform::IDENTITY);
        world.insert(entity, component.clone()).unwrap();
    }
    let mut extractor = MeshExtractor::default();
    assert_eq!(extractor.extract(&world), Err(MeshError::Capacity));
    assert!(extractor.draws().is_empty());
    assert_eq!(extractor.extract(&World::new()).unwrap().visible, 0);
}
