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
pub use quality_loop::QualityLoop;
pub use extrem_app::frame;
pub use extrem_app::lod;
pub use extrem_app::quality;
pub use extrem_app::{Stage, Time};
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
    is_hierarchically_visible, Camera, CameraPriority, Children, GlobalTransform, Name, Parent,
    Projection, Scene, SceneDocument, SceneFormatError, SceneNode, Velocity, Visibility,
};
pub use extrem_window::{WindowConfig, WindowError, WindowHost};

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
        Self::with_renderer(NullRenderer::default())
    }
}

impl Default for Engine<NullRenderer> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: RenderBackend> Engine<R> {
    pub fn with_renderer(renderer: R) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let mut render_graph = RenderGraph::new();
        render_graph.add_pass("main", std::iter::empty::<&str>());
        let config = EngineConfig::default();
        Self {
            app,
            renderer,
            config,
            render_graph,
            last_frame_stats: FrameStats::default(),
            last_render_passes: Vec::new(),
            quality: QualityLoop::new(config.target_delta_seconds),
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

    pub fn set_config(&mut self, config: EngineConfig) {
        self.config = config;
        self.quality.set_target_delta_seconds(config.target_delta_seconds);
    }

    pub fn render_graph(&self) -> &RenderGraph {
        &self.render_graph
    }

    pub fn render_graph_mut(&mut self) -> &mut RenderGraph {
        &mut self.render_graph
    }

    pub fn last_frame_stats(&self) -> FrameStats {
        self.last_frame_stats
    }

    pub fn last_render_passes(&self) -> &[String] {
        &self.last_render_passes
    }

    pub fn quality_loop(&self) -> &QualityLoop {
        &self.quality
    }

    pub fn quality_loop_mut(&mut self) -> &mut QualityLoop {
        &mut self.quality
    }

    pub fn frame(&mut self, now: Instant) -> UpdateReport {
        let report = self.app.update(now);
        self.quality.observe_cpu_frame_seconds(report.frame_delta_seconds);
        propagate_transforms(self.world_mut());
        let commands = extract_transforms(self.world());
        let camera = select_camera(self.world(), self.config.viewport_aspect);

        let info = FrameInfo {
            frame_index: report.frame_index,
            delta_seconds: report.frame_delta_seconds,
            camera_view_projection: camera.map(|(_, matrix)| matrix),
        };
        self.renderer.begin_frame(info);
        for command in commands {
            self.renderer.submit(command);
        }
        self.last_render_passes = self.render_graph.execution_order().unwrap_or_default();
        self.last_frame_stats = self.renderer.end_frame();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use extrem_math::Transform;
    use extrem_scene::GlobalTransform;

    #[test]
    fn engine_has_main_render_pass() {
        let engine = Engine::new();
        assert_eq!(engine.render_graph().execution_order().unwrap(), vec!["main"]);
    }

    #[test]
    fn frame_extracts_transform_commands() {
        let mut engine = Engine::new();
        engine.world_mut().spawn(Transform::IDENTITY);
        let report = engine.frame(Instant::now());
        assert_eq!(engine.last_frame_stats().submitted_commands, 1);
        assert_eq!(report.frame_index, 0);
    }

    #[test]
    fn frame_uses_propagated_global_transform() {
        let mut engine = Engine::new();
        let entity = engine.world_mut().spawn(Transform::IDENTITY);
        engine
            .world_mut()
            .insert(
                entity,
                GlobalTransform(Transform::from_translation([3.0, 4.0, 5.0])),
            )
            .expect("insert global");
        let _ = engine.frame(Instant::now());
        assert_eq!(engine.last_frame_stats().submitted_commands, 1);
    }
}
