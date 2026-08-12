use extrem_ecs::Entity;
use extrem_math::{Mat4, Vec3};
use std::fmt;

/// Information known when a frame begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub index: u64,
    pub delta_seconds: f32,
}

/// Render-side command extracted from the game world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderCommand {
    SetCamera {
        entity: Entity,
        view_projection: Mat4,
    },
    Transform {
        entity: Entity,
        translation: Vec3,
    },
}

/// Stable identifier for a render graph pass.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenderPassId(usize);

/// Render graph compilation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderGraphError {
    MissingPass(RenderPassId),
    Cycle(RenderPassId),
}

impl fmt::Display for RenderGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPass(id) => {
                write!(formatter, "render graph references missing pass {id:?}")
            }
            Self::Cycle(id) => write!(formatter, "render graph contains a cycle at pass {id:?}"),
        }
    }
}

impl std::error::Error for RenderGraphError {}

#[derive(Clone, Debug)]
struct RenderPass {
    name: String,
    dependencies: Vec<RenderPassId>,
}

/// Deterministic dependency graph for render passes.
#[derive(Clone, Debug, Default)]
pub struct RenderGraph {
    passes: Vec<RenderPass>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_pass(&mut self, name: impl Into<String>) -> RenderPassId {
        let id = RenderPassId(self.passes.len());
        self.passes.push(RenderPass {
            name: name.into(),
            dependencies: Vec::new(),
        });
        id
    }

    pub fn add_dependency(
        &mut self,
        pass: RenderPassId,
        dependency: RenderPassId,
    ) -> Result<(), RenderGraphError> {
        if self.passes.get(pass.0).is_none() {
            return Err(RenderGraphError::MissingPass(pass));
        }
        if self.passes.get(dependency.0).is_none() {
            return Err(RenderGraphError::MissingPass(dependency));
        }
        self.passes[pass.0].dependencies.push(dependency);
        Ok(())
    }

    pub fn pass_name(&self, pass: RenderPassId) -> Option<&str> {
        self.passes.get(pass.0).map(|pass| pass.name.as_str())
    }

    pub fn compile(&self) -> Result<Vec<RenderPassId>, RenderGraphError> {
        let mut states = vec![0_u8; self.passes.len()];
        let mut order = Vec::with_capacity(self.passes.len());
        for index in 0..self.passes.len() {
            visit_pass(index, self, &mut states, &mut order)?;
        }
        Ok(order)
    }
}

fn visit_pass(
    index: usize,
    graph: &RenderGraph,
    states: &mut [u8],
    order: &mut Vec<RenderPassId>,
) -> Result<(), RenderGraphError> {
    match states[index] {
        1 => return Err(RenderGraphError::Cycle(RenderPassId(index))),
        2 => return Ok(()),
        _ => {}
    }
    states[index] = 1;
    for dependency in &graph.passes[index].dependencies {
        if graph.passes.get(dependency.0).is_none() {
            return Err(RenderGraphError::MissingPass(*dependency));
        }
        visit_pass(dependency.0, graph, states, order)?;
    }
    states[index] = 2;
    order.push(RenderPassId(index));
    Ok(())
}

/// Basic statistics exposed by a renderer after a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub submitted_commands: usize,
    pub drawn_pixels: usize,
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
            drawn_pixels: 0,
        };
        self.last_stats = stats;
        self.frame = None;
        stats
    }
}

/// Minimal deterministic CPU renderer useful for tests, screenshots and CI.
#[derive(Clone, Debug)]
pub struct CpuRenderer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    submitted_commands: usize,
    drawn_pixels: usize,
    last_stats: FrameStats,
}

impl CpuRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_count = width.saturating_mul(height).saturating_mul(3) as usize;
        Self {
            width,
            height,
            pixels: vec![0; pixel_count],
            submitted_commands: 0,
            drawn_pixels: 0,
            last_stats: FrameStats::default(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.pixels
    }

    pub fn last_stats(&self) -> FrameStats {
        self.last_stats
    }

    pub fn save_ppm(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let mut output = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        output.extend_from_slice(&self.pixels);
        std::fs::write(path, output)
    }

    fn clear(&mut self) {
        for pixel in self.pixels.chunks_exact_mut(3) {
            pixel.copy_from_slice(&[18, 22, 30]);
        }
    }

    fn draw_marker(&mut self, x: f32, y: f32) {
        let center_x = ((x * 0.05 + 0.5) * self.width as f32) as i32;
        let center_y = ((0.5 - y * 0.05) * self.height as f32) as i32;
        for offset_y in -3..=3 {
            for offset_x in -3..=3 {
                let pixel_x = center_x + offset_x;
                let pixel_y = center_y + offset_y;
                if pixel_x < 0
                    || pixel_y < 0
                    || pixel_x >= self.width as i32
                    || pixel_y >= self.height as i32
                {
                    continue;
                }
                let index = ((pixel_y as u32 * self.width + pixel_x as u32) * 3) as usize;
                self.pixels[index..index + 3].copy_from_slice(&[92, 201, 255]);
                self.drawn_pixels += 1;
            }
        }
    }
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(640, 360)
    }
}

impl RenderBackend for CpuRenderer {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.clear();
        self.submitted_commands = 0;
        self.drawn_pixels = 0;
    }

    fn submit(&mut self, command: RenderCommand) {
        self.submitted_commands = self.submitted_commands.saturating_add(1);
        if let RenderCommand::Transform { translation, .. } = command {
            self.draw_marker(translation.x, translation.y);
        }
    }

    fn end_frame(&mut self) -> FrameStats {
        self.last_stats = FrameStats {
            submitted_commands: self.submitted_commands,
            drawn_pixels: self.drawn_pixels,
        };
        self.last_stats
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CpuRenderer, FrameInfo, RenderBackend, RenderCommand, RenderGraph, RenderGraphError,
    };
    use extrem_ecs::Entity;
    use extrem_math::Vec3;

    #[test]
    fn graph_compiles_dependencies_before_consumers() {
        let mut graph = RenderGraph::new();
        let clear = graph.add_pass("clear");
        let opaque = graph.add_pass("opaque");
        let ui = graph.add_pass("ui");
        graph.add_dependency(opaque, clear).expect("dependency");
        graph.add_dependency(ui, opaque).expect("dependency");

        let order = graph.compile().expect("acyclic graph");
        assert_eq!(order, vec![clear, opaque, ui]);
    }

    #[test]
    fn graph_rejects_cycles() {
        let mut graph = RenderGraph::new();
        let a = graph.add_pass("a");
        let b = graph.add_pass("b");
        graph.add_dependency(a, b).expect("dependency");
        graph.add_dependency(b, a).expect("dependency");
        assert!(matches!(graph.compile(), Err(RenderGraphError::Cycle(_))));
    }

    #[test]
    fn cpu_renderer_draws_transform_markers() {
        let mut renderer = CpuRenderer::new(32, 32);
        renderer.begin_frame(FrameInfo {
            index: 1,
            delta_seconds: 1.0 / 60.0,
        });
        renderer.submit(RenderCommand::Transform {
            entity: Entity::from_raw(1),
            translation: Vec3::ZERO,
        });
        let stats = renderer.end_frame();
        assert_eq!(stats.submitted_commands, 1);
        assert!(stats.drawn_pixels > 0);
    }
}
