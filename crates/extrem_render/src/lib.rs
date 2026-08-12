use extrem_ecs::Entity;
use extrem_math::Vec3;

/// Information known when a frame begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub index: u64,
    pub delta_seconds: f32,
}

/// Render-side command extracted from the game world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderCommand {
    Transform { entity: Entity, translation: Vec3 },
}

/// Basic statistics exposed by a renderer after a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub submitted_commands: usize,
}

/// Backend boundary. A future `wgpu`, Vulkan or software backend can implement it.
pub trait RenderBackend {
    fn begin_frame(&mut self, info: FrameInfo);
    fn submit(&mut self, command: RenderCommand);
    fn end_frame(&mut self) -> FrameStats;
}

/// Renderer used by tests, tools and the first headless engine executable.
#[derive(Clone, Debug, Default)]
pub struct NullRenderer {
    frame: Option<FrameInfo>,
    submitted_commands: usize,
    last_stats: FrameStats,
}

impl NullRenderer {
    pub fn last_stats(&self) -> FrameStats {
        self.last_stats
    }
}

impl RenderBackend for NullRenderer {
    fn begin_frame(&mut self, info: FrameInfo) {
        self.frame = Some(info);
        self.submitted_commands = 0;
    }

    fn submit(&mut self, _command: RenderCommand) {
        self.submitted_commands = self.submitted_commands.saturating_add(1);
    }

    fn end_frame(&mut self) -> FrameStats {
        let stats = FrameStats {
            submitted_commands: self.submitted_commands,
        };
        self.last_stats = stats;
        self.frame = None;
        stats
    }
}
