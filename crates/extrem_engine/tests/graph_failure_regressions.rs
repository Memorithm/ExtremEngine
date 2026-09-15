//! Regression reproductions that also compile against the pre-fix tick API.
use extrem_engine::{Engine, EngineConfig};
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand};
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Default)]
struct Probe {
    begins: usize,
    submits: usize,
    ends: usize,
}

impl RenderBackend for Probe {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.begins += 1;
    }

    fn submit(&mut self, _command: RenderCommand) {
        self.submits += 1;
    }

    fn end_frame(&mut self) -> FrameStats {
        self.ends += 1;
        FrameStats::default()
    }
}

fn invalid_engine() -> Engine<Probe> {
    let mut engine = Engine::with_renderer(Probe::default(), EngineConfig::default());
    let graph = engine.render_graph_mut();
    let pass = graph.add_pass("cyclic");
    graph.add_dependency(pass, pass).unwrap();
    engine
}

#[test]
fn cyclic_tick_does_not_unwind() {
    let mut engine = invalid_engine();
    let outcome = catch_unwind(AssertUnwindSafe(|| engine.tick(0.25)));
    assert!(
        outcome.is_ok(),
        "a graph error must be returned, not panicked"
    );
}

#[test]
fn failed_compile_does_not_advance_time_or_open_backend() {
    let mut engine = invalid_engine();
    let time = engine.app().time();
    // Deliberately inspect side effects even when the old implementation panics.
    let _outcome = catch_unwind(AssertUnwindSafe(|| engine.tick(0.25)));
    assert_eq!(engine.app().time(), time);
    let renderer = engine.renderer();
    assert_eq!(
        (renderer.begins, renderer.submits, renderer.ends),
        (0, 0, 0)
    );
}

#[test]
fn run_for_rejects_a_cyclic_graph_without_unwinding() {
    let mut engine = invalid_engine();
    let outcome = catch_unwind(AssertUnwindSafe(|| engine.run_for(3)));
    assert!(
        outcome.is_ok(),
        "batch execution must propagate the graph error"
    );
    assert_eq!(engine.app().time().frame, 0);
}
