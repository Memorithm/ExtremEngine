use extrem_ecs::Entity;
use extrem_gpu::{GpuContext, GpuError, SurfaceFrame, SurfaceFrameStatus, SurfaceTarget};
use extrem_math::{Mat4, Vec3};
use extrem_mesh::MeshData;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub index: u64,
    pub delta_seconds: f32,
}

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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderPassId(usize);

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledRenderGraph {
    pub version: u64,
    pub execution_order: Vec<RenderPassId>,
}

/// Backend-neutral deterministic dependency graph with topology-versioned plan caching.
#[derive(Clone, Debug, Default)]
pub struct RenderGraph {
    passes: Vec<RenderPass>,
    version: u64,
    cached_plan: Option<CompiledRenderGraph>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn add_pass(&mut self, name: impl Into<String>) -> RenderPassId {
        let id = RenderPassId(self.passes.len());
        self.passes.push(RenderPass {
            name: name.into(),
            dependencies: Vec::new(),
        });
        self.invalidate();
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
        if !self.passes[pass.0].dependencies.contains(&dependency) {
            self.passes[pass.0].dependencies.push(dependency);
            self.passes[pass.0].dependencies.sort_unstable();
            self.invalidate();
        }
        Ok(())
    }

    pub fn pass_name(&self, pass: RenderPassId) -> Option<&str> {
        self.passes.get(pass.0).map(|pass| pass.name.as_str())
    }

    pub fn cached_plan(&self) -> Option<&CompiledRenderGraph> {
        self.cached_plan.as_ref()
    }

    pub fn compile(&mut self) -> Result<CompiledRenderGraph, RenderGraphError> {
        if let Some(plan) = &self.cached_plan {
            if plan.version == self.version {
                return Ok(plan.clone());
            }
        }

        let mut states = vec![0_u8; self.passes.len()];
        let mut order = Vec::with_capacity(self.passes.len());
        for index in 0..self.passes.len() {
            visit_pass(index, self, &mut states, &mut order)?;
        }
        let plan = CompiledRenderGraph {
            version: self.version,
            execution_order: order,
        };
        self.cached_plan = Some(plan.clone());
        Ok(plan)
    }

    fn invalidate(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.cached_plan = None;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub submitted_commands: usize,
    pub drawn_pixels: usize,
}

pub trait RenderBackend {
    fn begin_frame(&mut self, info: FrameInfo);
    fn submit(&mut self, command: RenderCommand);
    fn end_frame(&mut self) -> FrameStats;
}

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
        if !x.is_finite() || !y.is_finite() {
            return;
        }
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
                self.drawn_pixels = self.drawn_pixels.saturating_add(1);
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
    fn graph_compiles_dependencies_before_consumers_and_reuses_cache() {
        let mut graph = RenderGraph::new();
        let clear = graph.add_pass("clear");
        let opaque = graph.add_pass("opaque");
        let ui = graph.add_pass("ui");
        graph.add_dependency(opaque, clear).expect("dependency");
        graph.add_dependency(ui, opaque).expect("dependency");
        let compiled = graph.compile().expect("acyclic graph");
        assert_eq!(compiled.execution_order, vec![clear, opaque, ui]);
        assert_eq!(graph.cached_plan(), Some(&compiled));
        assert_eq!(graph.compile().expect("cached graph"), compiled);
    }

    #[test]
    fn duplicate_dependency_does_not_invalidate_cache() {
        let mut graph = RenderGraph::new();
        let a = graph.add_pass("a");
        let b = graph.add_pass("b");
        graph.add_dependency(b, a).expect("dependency");
        let compiled = graph.compile().expect("compile");
        graph.add_dependency(b, a).expect("duplicate dependency");
        assert_eq!(graph.cached_plan(), Some(&compiled));
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

#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct MeshUniforms {
    view_projection: [f32; 16],
    model: [f32; 16],
    color: [f32; 4],
}

#[derive(Debug)]
pub enum MeshRenderError {
    Gpu(GpuError),
    InvalidMesh(String),
}

impl fmt::Display for MeshRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gpu(error) => write!(formatter, "GPU error: {error}"),
            Self::InvalidMesh(message) => write!(formatter, "invalid mesh: {message}"),
        }
    }
}

impl std::error::Error for MeshRenderError {}

impl From<GpuError> for MeshRenderError {
    fn from(error: GpuError) -> Self {
        Self::Gpu(error)
    }
}

const MESH_SHADER: &str = r#"
struct Uniforms {
    view_projection: mat4x4f,
    model: mat4x4f,
    color: vec4f,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
};

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_position = uniforms.model * vec4f(input.position, 1.0);
    output.position = uniforms.view_projection * world_position;
    let light_dir = normalize(vec3f(0.5, 1.0, 0.3));
    let diffuse = max(dot(normalize(input.normal), light_dir), 0.0);
    let ambient = 0.3;
    let lighting = ambient + diffuse * 0.7;
    output.color = vec4f(uniforms.color.rgb * lighting, uniforms.color.a);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
"#;

pub struct MeshRenderer {
    context: GpuContext,
    surface: SurfaceTarget,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl MeshRenderer {
    pub fn new(
        context: GpuContext,
        surface: SurfaceTarget,
        mesh: &MeshData,
    ) -> Result<Self, MeshRenderError> {
        let device = context.device();
        let format = surface.format();
        let width = surface.width();
        let height = surface.height();

        let vertex_data: &[u8] = bytemuck::cast_slice(&mesh.vertices);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh vertex buffer"),
            size: vertex_data.len() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        context.queue().write_buffer(&vertex_buffer, 0, vertex_data);

        let index_data: &[u8] = bytemuck::cast_slice(&mesh.indices);
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh index buffer"),
            size: index_data.len() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });
        context.queue().write_buffer(&index_buffer, 0, index_data);

        let index_count = mesh.index_count();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh uniform buffer"),
            size: std::mem::size_of::<MeshUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(MESH_SHADER)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 24,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let (depth_texture, depth_view) = Self::create_depth_texture(device, width, height);

        Ok(Self {
            context,
            surface,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            uniform_buffer,
            bind_group,
            depth_texture,
            depth_view,
            width,
            height,
        })
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh depth texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        let resized = self.surface.resize(self.context.device(), width, height);
        if resized {
            self.width = width;
            self.height = height;
            let (depth_texture, depth_view) =
                Self::create_depth_texture(self.context.device(), width, height);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
        }
        resized
    }

    pub fn draw_mesh(
        &self,
        view_projection: Mat4,
        model: Mat4,
        color: [f32; 4],
    ) -> Result<(), MeshRenderError> {
        let uniforms = MeshUniforms {
            view_projection: view_projection.data,
            model: model.data,
            color,
        };
        self.context.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );

        let frame = self.surface.acquire_frame();
        let (texture, status) = match frame {
            SurfaceFrame::Renderable { texture, status } => (texture, status),
            SurfaceFrame::Unavailable(_) => return Ok(()),
        };

        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("mesh draw encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.055,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        self.context.queue().submit(Some(encoder.finish()));
        self.context.queue().present(texture);

        if status == SurfaceFrameStatus::Suboptimal {
            self.surface.reconfigure(self.context.device());
        }

        Ok(())
    }

    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    pub fn surface(&self) -> &SurfaceTarget {
        &self.surface
    }
}

impl RenderBackend for MeshRenderer {
    fn begin_frame(&mut self, _info: FrameInfo) {}

    fn submit(&mut self, _command: RenderCommand) {}

    fn end_frame(&mut self) -> FrameStats {
        FrameStats {
            submitted_commands: 0,
            drawn_pixels: 0,
        }
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    use extrem_mesh::unit_cube;

    #[test]
    fn mesh_uniforms_size_matches_shader() {
        let size = std::mem::size_of::<MeshUniforms>();
        assert_eq!(size, 144);
    }

    #[test]
    fn mesh_renderer_compiles_with_unit_cube() {
        let _mesh = unit_cube();
        assert_eq!(_mesh.vertex_count(), 24);
        assert_eq!(_mesh.index_count(), 36);
    }
}
