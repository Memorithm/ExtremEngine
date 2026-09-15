use extrem_engine::{Engine, EngineConfig, Input, KeyCode, RenderGraphError, Stage, Time};
use extrem_math::Transform;
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand, RenderGraph, RenderPassId};
use extrem_scene::{GlobalTransform, TransformPropagationStats};

#[derive(Clone, Debug, Default, PartialEq)]
struct Probe {
    frames: Vec<FrameInfo>,
    commands: Vec<RenderCommand>,
    ended: usize,
    open: bool,
}

impl RenderBackend for Probe {
    fn begin_frame(&mut self, info: FrameInfo) {
        assert!(!self.open, "previous frame was not closed");
        self.open = true;
        self.frames.push(info);
        self.commands.clear();
    }

    fn submit(&mut self, command: RenderCommand) {
        assert!(self.open);
        self.commands.push(command);
    }

    fn end_frame(&mut self) -> FrameStats {
        assert!(self.open);
        self.open = false;
        self.ended += 1;
        FrameStats {
            submitted_commands: self.commands.len(),
            drawn_pixels: 0,
        }
    }
}

fn corrupt(engine: &mut Engine<Probe>) -> RenderPassId {
    let graph = engine.render_graph_mut();
    let bad = graph.add_pass("bad");
    graph.add_dependency(bad, bad).unwrap();
    bad
}

#[test]
fn rejection_preserves_every_stage_input_time_and_world_before_startup() {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    engine.world_mut().insert_resource([0_u32; 5]);
    for (index, stage) in [
        Stage::Startup,
        Stage::FixedUpdate,
        Stage::Update,
        Stage::PostUpdate,
        Stage::Render,
    ]
    .into_iter()
    .enumerate()
    {
        engine.app_mut().add_systems(stage, move |world, _| {
            world.get_resource_mut::<[u32; 5]>().unwrap()[index] += 1;
        });
    }
    let entity = engine.world_mut().spawn(Transform::IDENTITY);
    let mut input = Input::default();
    input.keys.press(KeyCode::Space);
    engine.set_input_snapshot(&input);
    let initial_time = engine.app().time();
    let initial_extraction = engine.last_extraction_stats();
    let initial_propagation = *engine.world().get_resource::<TransformPropagationStats>().unwrap();
    let bad = corrupt(&mut engine);
    for delta in [0.0, 0.25, f32::INFINITY] {
        assert_eq!(engine.tick(delta), Err(RenderGraphError::Cycle(bad)));
        assert_eq!(engine.app().time(), initial_time);
        assert_eq!(engine.world().get_resource::<Time>(), Some(&initial_time));
        assert_eq!(engine.world().get_resource::<[u32; 5]>(), Some(&[0; 5]));
        assert_eq!(engine.world().get::<Transform>(entity), Some(&Transform::IDENTITY));
        assert!(engine.world().get::<GlobalTransform>(entity).is_none());
        assert!(engine.world().get_resource::<Input>().unwrap().keys.just_pressed(KeyCode::Space));
        assert_eq!(engine.renderer(), &Probe::default());
        assert_eq!(engine.last_extraction_stats(), initial_extraction);
        assert_eq!(engine.world().get_resource::<TransformPropagationStats>(), Some(&initial_propagation));
    }
    engine.render_graph_mut().remove_dependency(bad, bad).unwrap();
    let report = engine.tick(0.25).unwrap();
    assert_eq!(report.frame, 1);
    assert_eq!(report.elapsed_seconds, 0.25);
    assert_eq!(engine.world().get_resource::<[u32; 5]>(), Some(&[1, 8, 1, 1, 1]));
    assert!(!engine.world().get_resource::<Input>().unwrap().keys.just_pressed(KeyCode::Space));
    assert_eq!(engine.renderer().ended, 1);
    assert!(!engine.renderer().open);
}

#[test]
fn rejection_after_success_preserves_last_frame_and_retries_once_after_repair() {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    let entity = engine.world_mut().spawn(Transform::IDENTITY);
    engine.tick(0.125).unwrap();
    let time = engine.app().time();
    let renderer = engine.renderer().clone();
    let frame_stats = engine.last_frame_stats();
    let extraction_stats = engine.last_extraction_stats();
    let passes = engine.last_render_passes().to_vec();
    let global = *engine.world().get::<GlobalTransform>(entity).unwrap();
    let bad = corrupt(&mut engine);
    for _ in 0..3 {
        assert_eq!(engine.tick(1000.0), Err(RenderGraphError::Cycle(bad)));
        assert_eq!(engine.app().time(), time);
        assert_eq!(engine.renderer(), &renderer);
        assert_eq!(engine.last_frame_stats(), frame_stats);
        assert_eq!(engine.last_extraction_stats(), extraction_stats);
        assert_eq!(engine.last_render_passes(), passes);
        assert_eq!(engine.world().get::<GlobalTransform>(entity), Some(&global));
        assert!(engine.render_graph().cached_plan().is_none());
    }
    engine.render_graph_mut().remove_dependency(bad, bad).unwrap();
    let report = engine.tick(0.125).unwrap();
    assert_eq!((report.frame, report.elapsed_seconds), (2, 0.25));
    assert_eq!(engine.renderer().ended, 2);
    assert_eq!(engine.last_render_passes(), ["clear", "main", "ui", "bad"]);
}

#[test]
fn replacing_graph_with_equal_version_does_not_bypass_validation() {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    engine.tick(0.0).unwrap();
    let previous = engine.renderer().clone();
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("replacement-a");
    let b = graph.add_pass("replacement-b");
    graph.add_pass("replacement-c");
    graph.add_dependency(a, b).unwrap();
    graph.add_dependency(b, a).unwrap();
    assert_eq!(graph.version(), engine.render_graph().version());
    *engine.render_graph_mut() = graph;
    assert_eq!(engine.tick(0.0), Err(RenderGraphError::Cycle(a)));
    assert_eq!(engine.renderer(), &previous);
    engine.render_graph_mut().remove_dependency(b, a).unwrap();
    assert_eq!(engine.tick(0.0).unwrap().frame, 2);
    assert_eq!(engine.last_render_passes(), ["replacement-b", "replacement-a", "replacement-c"]);
}

#[test]
fn run_for_propagates_error_and_zero_frames_remains_a_noop() {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    let bad = corrupt(&mut engine);
    assert!(engine.run_for(0).unwrap().is_empty());
    assert_eq!(engine.run_for(3), Err(RenderGraphError::Cycle(bad)));
    assert_eq!(engine.app().time().frame, 0);
    assert_eq!(engine.renderer(), &Probe::default());
    engine.render_graph_mut().remove_dependency(bad, bad).unwrap();
    let reports = engine.run_for(3).unwrap();
    assert_eq!(reports.iter().map(|report| report.frame).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(engine.renderer().ended, 3);
    assert!(!engine.renderer().open);
}

#[test]
fn empty_replacement_graph_still_balances_backend_callbacks() {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    *engine.render_graph_mut() = RenderGraph::new();
    engine.tick(0.0).unwrap();
    assert!(engine.last_render_passes().is_empty());
    assert_eq!(engine.renderer().frames.len(), 1);
    assert_eq!(engine.renderer().ended, 1);
    assert!(!engine.renderer().open);
}
