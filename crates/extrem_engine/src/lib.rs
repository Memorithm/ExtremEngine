use std::time::Instant;

use extrem_app::{App, MinimalPlugins, UpdateReport};
use extrem_ecs::World;
use extrem_render::{
    FrameInfo, FrameStats, NullRenderer, RenderBackend, RenderCommand, RenderGraph,
};
use extrem_scene::propagate_transforms;

mod extract;
mod quality_loop;

pub use extract::{extract_transforms, select_camera};
pub use extrem_app::frame;
pub use extrem_app::lod;
pub use extrem_app::quality;
pub use extrem_app::{Stage, Time};
pub use extrem_assets::geometry_codec::{
    decode_with, GeometryCodec, GeometryDecodeError, GeometryDecoder,
};
pub use extrem_assets::{AssetError, AssetId, AssetKey, AssetPathError, Assets, Handle};
pub use extrem_audio::{AudioBackend, AudioCommand, NullAudioBackend};
pub use extrem_ecs::{Entity, WorldError};
pub use extrem_editor::{EditorCommand, EditorError, EditorState, InspectorSnapshot};
pub use extrem_gpu::{GpuContext, GpuError, SurfaceFrameStatus, SurfaceTarget, WgpuPresenter};
pub use extrem_input::{ButtonInput, Input, KeyCode, MouseButton, MouseState};
pub use extrem_physics::{
    BodyType, BoxCollider, Gravity, PhysicsError, PhysicsPlugin, PhysicsStats, RigidBody,
};
pub use extrem_scene::{
    Camera, CameraPriority, Children, GlobalTransform, Name, Parent, Projection, Scene,
    SceneDocument, SceneFormatError, SceneNode, Velocity, Visibility, is_hierarchically_visible,
};
pub use extrem_window::{WindowConfig, WindowError, WindowHost};
pub use quality_loop::QualityLoop;

/// Adapter from the low-level WGPU presenter to ExtremEngine's backend contract.
///
/// The current GPU path intentionally draws a built-in validation triangle. World mesh/material
/// rendering is not claimed by this type yet; submitted world commands are counted for diagnostics.
pub struct WgpuRenderer {
    presenter: WgpuPresenter,
    submitted_commands: usize,
    last_stats: FrameStats,
    last_surface_status: Option<SurfaceFrameStatus>,
}

impl WgpuRenderer {
    pub fn new(presenter: WgpuPresenter) -> Self {
        Self {
            presenter,
            submitted_commands: 0,
            last_stats: FrameStats::default(),
            last_surface_status: None,
        }
    }

    pub fn presenter(&self) -> &WgpuPresenter {
        &self.presenter
    }

    pub fn presenter_mut(&mut self) -> &mut WgpuPresenter {
        &mut self.presenter
    }

    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        self.presenter.resize(width, height)
    }

    pub fn last_surface_status(&self) -> Option<SurfaceFrameStatus> {
        self.last_surface_status
    }
}

impl RenderBackend for WgpuRenderer {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.submitted_commands = 0;
    }

    fn submit(&mut self, _command: RenderCommand) {
        self.submitted_commands = self.submitted_commands.saturating_add(1);
    }

    fn end_frame(&mut self) -> FrameStats {
        self.last_surface_status = Some(self.presenter.render_validation_frame());
        self.last_stats = FrameStats {
            submitted_commands: self.submitted_commands,
            drawn_pixels: 0,
        };
        self.last_stats
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineConfig {
    pub target_delta_seconds: f32,
    pub fixed_delta_seconds: f32,
    pub max_fixed_steps_per_frame: u32,
    pub viewport_aspect: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            target_delta_seconds: 1.0 / 60.0,
            fixed_delta_seconds: 1.0 / 60.0,
            max_fixed_steps_per_frame: 8,
            viewport_aspect: 16.0 / 9.0,
        }
    }
}

/// The engine owns the application lifecycle, persistent render graph and a replaceable renderer.
pub struct Engine<R: RenderBackend = NullRenderer> {
    app: App,
    renderer: R,
    config: EngineConfig,
    render_graph: RenderGraph,
    last_frame_stats: FrameStats,
    last_render_passes: Vec<String>,
    quality: QualityLoop,
}

impl Engine<NullRenderer> {
    pub fn new() -> Self {
        Self::with_renderer(NullRenderer::default(), EngineConfig::default())
    }
}

impl Default for Engine<NullRenderer> {
    fn default() -> Self {
        Self::new()
    }
}

fn default_render_graph() -> RenderGraph {
    let mut graph = RenderGraph::new();
    let clear = graph.add_pass("clear");
    let main = graph.add_pass("main");
    let ui = graph.add_pass("ui");
    graph
        .add_dependency(main, clear)
        .expect("default graph pass IDs are valid");
    graph
        .add_dependency(ui, main)
        .expect("default graph pass IDs are valid");
    graph
}

impl<R: RenderBackend> Engine<R> {
    pub fn with_renderer(renderer: R, config: EngineConfig) -> Self {
        let mut app = App::new();
        app.add_plugin(MinimalPlugins);
        app.add_plugin(extrem_physics::PhysicsPlugin);
        app.set_fixed_timestep(config.fixed_delta_seconds)
            .set_max_fixed_steps_per_frame(config.max_fixed_steps_per_frame)
            .add_systems(extrem_app::Stage::PostUpdate, |world, _| {
                propagate_transforms(world)
            });
        app.world_mut().insert_resource(Input::default());

        Self {
            app,
            renderer,
            config,
            render_graph: default_render_graph(),
            last_frame_stats: FrameStats::default(),
            last_render_passes: Vec::new(),
            quality: QualityLoop::from_target_delta(config.target_delta_seconds),
        }
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn world(&self) -> &World {
        self.app.world()
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub fn renderer(&self) -> &R {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut R {
        &mut self.renderer
    }

    pub fn config(&self) -> EngineConfig {
        self.config
    }

    pub fn render_graph(&self) -> &RenderGraph {
        &self.render_graph
    }

    pub fn render_graph_mut(&mut self) -> &mut RenderGraph {
        &mut self.render_graph
    }

    pub fn quality(&self) -> QualityLoop {
        self.quality
    }

    pub fn resolution_scale(&self) -> f32 {
        self.quality.scale()
    }

    /// Copies the current native input snapshot into the ECS before a frame update.
    pub fn set_input_snapshot(&mut self, input: &Input) {
        self.world_mut().insert_resource(input.clone());
    }

    pub fn tick(&mut self, delta_seconds: f32) -> UpdateReport {
        let started = Instant::now();
        let report = self.app.update(delta_seconds);
        self.renderer.begin_frame(FrameInfo {
            index: report.frame,
            delta_seconds: report.delta_seconds,
        });

        let compiled = self
            .render_graph
            .compile()
            .expect("the built-in render graph must remain acyclic");
        self.last_render_passes = compiled
            .execution_order
            .iter()
            .filter_map(|pass| self.render_graph.pass_name(*pass).map(str::to_owned))
            .collect();

        if let Some((entity, view_projection)) =
            extract::select_camera(self.world(), self.config.viewport_aspect)
        {
            self.renderer.submit(RenderCommand::SetCamera {
                entity,
                view_projection,
            });
        }

        for command in extract::extract_transforms(self.world()) {
            self.renderer.submit(command);
        }

        self.last_frame_stats = self.renderer.end_frame();
        if let Some(input) = self.world_mut().get_resource_mut::<Input>() {
            input.end_frame();
        }
        self.quality.observe_cpu(started.elapsed());
        report
    }

    pub fn run_for(&mut self, frames: usize) -> Vec<UpdateReport> {
        (0..frames)
            .map(|_| self.tick(self.config.target_delta_seconds))
            .collect()
    }

    pub fn last_frame_stats(&self) -> FrameStats {
        self.last_frame_stats
    }

    pub fn last_render_passes(&self) -> &[String] {
        &self.last_render_passes
    }
}

#[cfg(test)]
mod tests {
    use super::{Camera, CameraPriority, Engine, Input, KeyCode, Stage, Visibility};
    use extrem_math::{Transform, Vec3};
    use extrem_scene::Velocity;

    #[test]
    fn engine_updates_and_extracts_transforms() {
        let mut engine = Engine::new();
        let entity = engine.world_mut().spawn_empty();
        engine
            .world_mut()
            .insert(entity, Transform::default())
            .expect("entity is alive");
        engine
            .world_mut()
            .insert(entity, Velocity(Vec3::new(1.0, 0.0, 0.0)))
            .expect("entity is alive");
        engine.app_mut().add_systems(Stage::Update, |world, time| {
            let entities: Vec<_> = world.iter::<Velocity>().map(|(entity, _)| entity).collect();
            for entity in entities {
                let velocity = world.get::<Velocity>(entity).expect("velocity").0;
                if let Some(transform) = world.get_mut::<Transform>(entity) {
                    transform.translation += velocity * time.delta_seconds;
                }
            }
        });

        engine.run_for(2);
        let position = engine
            .world()
            .get::<Transform>(entity)
            .expect("transform")
            .translation;
        assert!((position.x - 2.0 / 60.0).abs() < 0.000_01);
        assert_eq!(engine.last_frame_stats().submitted_commands, 1);
        assert!(engine.quality().stats().count() >= 2);
    }

    #[test]
    fn engine_extracts_active_camera_and_closes_input_frame() {
        let mut engine = Engine::new();
        let entity = engine.world_mut().spawn_empty();
        engine
            .world_mut()
            .insert(entity, Transform::default())
            .expect("entity is alive");
        engine
            .world_mut()
            .insert(entity, Camera::default())
            .expect("entity is alive");
        let mut input = Input::default();
        input.keys.press(KeyCode::Space);
        engine.set_input_snapshot(&input);
        engine.app_mut().add_systems(Stage::Update, |world, _| {
            assert!(
                world
                    .get_resource::<Input>()
                    .expect("input resource")
                    .keys
                    .just_pressed(KeyCode::Space)
            );
        });

        engine.tick(1.0 / 60.0);
        assert_eq!(engine.last_frame_stats().submitted_commands, 2);
        assert_eq!(engine.last_render_passes(), ["clear", "main", "ui"]);
        assert!(
            !engine
                .world()
                .get_resource::<Input>()
                .expect("input resource")
                .keys
                .just_pressed(KeyCode::Space)
        );
    }

    #[test]
    fn default_render_graph_is_persistent_across_ticks() {
        let mut engine = Engine::new();
        let version = engine.render_graph().version();
        assert!(engine.render_graph().cached_plan().is_none());
        engine.tick(1.0 / 60.0);
        assert!(engine.render_graph().cached_plan().is_some());
        engine.tick(1.0 / 60.0);
        assert_eq!(engine.render_graph().version(), version);
    }

    #[test]
    fn camera_selection_uses_lowest_entity_as_deterministic_tie_break() {
        let mut engine = Engine::new();
        let first = engine.world_mut().spawn(Transform::IDENTITY);
        let second = engine.world_mut().spawn(Transform::IDENTITY);
        engine
            .world_mut()
            .insert(second, Camera::default())
            .expect("camera");
        engine
            .world_mut()
            .insert(first, Camera::default())
            .expect("camera");
        engine.tick(1.0 / 60.0);
        assert!(first < second);
        assert_eq!(engine.last_frame_stats().submitted_commands, 3);
    }

    #[test]
    fn hidden_entity_is_not_submitted() {
        let mut engine = Engine::new();
        let _visible = engine.world_mut().spawn(Transform::IDENTITY);
        let hidden = engine.world_mut().spawn(Transform::IDENTITY);
        engine
            .world_mut()
            .insert(hidden, Visibility(false))
            .expect("visibility");
        engine.tick(1.0 / 60.0);
        assert_eq!(engine.last_frame_stats().submitted_commands, 1);
    }

    #[test]
    fn camera_priority_overrides_entity_order() {
        let mut engine = Engine::new();
        let first = engine.world_mut().spawn(Transform::IDENTITY);
        let second = engine.world_mut().spawn(Transform::IDENTITY);
        engine
            .world_mut()
            .insert(first, Camera::default())
            .expect("camera");
        engine
            .world_mut()
            .insert(second, Camera::default())
            .expect("camera");
        engine
            .world_mut()
            .insert(second, CameraPriority(5))
            .expect("priority");
        engine.tick(1.0 / 60.0);
        assert!(first < second);
        assert_eq!(engine.last_frame_stats().submitted_commands, 3);
    }
}
