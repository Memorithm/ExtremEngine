use extrem_engine::{Engine, RenderGraphError, RenderPlanPreparationStats};
use extrem_render::RenderGraph;

#[test]
fn stable_ticks_and_noop_access_reuse_pass_names() {
    let mut engine = Engine::new();
    assert_eq!(engine.last_plan_preparation_stats(), RenderPlanPreparationStats::default());
    engine.tick(0.0).unwrap();
    assert_eq!(engine.last_plan_preparation_stats(), RenderPlanPreparationStats {
        passes: 3,
        refreshed_names: true,
        copied_name_bytes: 11,
    });
    let names_ptr = engine.last_render_passes().as_ptr();
    let string_ptr = engine.last_render_passes()[0].as_ptr();
    for _ in 0..16 {
        let _version = engine.render_graph_mut().version();
        engine.tick(0.0).unwrap();
        let stats = engine.last_plan_preparation_stats();
        assert_eq!(stats.passes, 3);
        assert!(!stats.refreshed_names);
        assert_eq!(stats.copied_name_bytes, 0);
        assert_eq!(engine.last_render_passes().as_ptr(), names_ptr);
        assert_eq!(engine.last_render_passes()[0].as_ptr(), string_ptr);
    }
}

#[test]
fn replacing_same_version_graph_changes_names_and_order_in_actual_tick() {
    let mut engine = Engine::new();
    engine.tick(0.0).unwrap();
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("new-a");
    let b = graph.add_pass("new-b");
    let c = graph.add_pass("new-c");
    graph.add_dependency(a, b).unwrap();
    graph.add_dependency(b, c).unwrap();
    assert_eq!(engine.render_graph().version(), graph.version());
    *engine.render_graph_mut() = graph;
    engine.tick(0.0).unwrap();
    assert_eq!(engine.last_render_passes(), ["new-c", "new-b", "new-a"]);
    assert!(engine.last_plan_preparation_stats().refreshed_names);
    engine.tick(0.0).unwrap();
    assert!(!engine.last_plan_preparation_stats().refreshed_names);
}

#[test]
fn rejected_tick_preserves_preparation_diagnostics_and_repairs_once() {
    let mut engine = Engine::new();
    engine.tick(0.1).unwrap();
    let previous = engine.last_plan_preparation_stats();
    let time = engine.app().time();
    let bad = engine.render_graph_mut().add_pass("bad");
    engine.render_graph_mut().add_dependency(bad, bad).unwrap();
    for _ in 0..3 {
        assert_eq!(engine.tick(0.1), Err(RenderGraphError::Cycle(bad)));
        assert_eq!(engine.last_plan_preparation_stats(), previous);
        assert_eq!(engine.last_render_passes(), ["clear", "main", "ui"]);
        assert_eq!(engine.app().time(), time);
    }
    engine.render_graph_mut().remove_dependency(bad, bad).unwrap();
    engine.tick(0.1).unwrap();
    assert!(engine.last_plan_preparation_stats().refreshed_names);
    assert_eq!(engine.last_render_passes(), ["clear", "main", "ui", "bad"]);
    engine.tick(0.0).unwrap();
    assert!(!engine.last_plan_preparation_stats().refreshed_names);
}

#[test]
fn cloned_graph_replacement_reuses_only_unchanged_snapshot() {
    let mut engine = Engine::new();
    engine.tick(0.0).unwrap();
    *engine.render_graph_mut() = engine.render_graph().clone();
    engine.tick(0.0).unwrap();
    assert!(!engine.last_plan_preparation_stats().refreshed_names);
    *engine.render_graph_mut() = RenderGraph::new();
    engine.tick(0.0).unwrap();
    assert!(engine.last_plan_preparation_stats().refreshed_names);
    assert!(engine.last_render_passes().is_empty());
    assert_eq!(engine.last_plan_preparation_stats().copied_name_bytes, 0);
}
