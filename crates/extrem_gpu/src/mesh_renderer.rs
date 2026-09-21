//! Native indexed mesh pipeline shared by offscreen and existing surface targets.
use crate::mesh_data::{
    MAX_FRAME_DRAWS, MAX_GEOMETRY_BYTES, MAX_RESIDENT_MESHES, MeshData, MeshDraw, MeshError,
    MeshLight, validate_extent, validate_lit_frame,
};
use crate::mesh_safety::GpuScopes;
use crate::{GpuContext, SurfaceFrame, SurfaceFrameStatus, SurfaceTarget};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, mpsc};
use std::time::Duration;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const INSTANCE_STRIDE: usize = 96;
const FRAME_UNIFORM_BYTES: usize = 128;
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Submission observations, not GPU elapsed time or completed-on-screen evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshFrameReport {
    pub submitted: bool,
    pub surface_status: Option<SurfaceFrameStatus>,
    /// Logical mesh instances provided by the caller before frustum culling.
    pub draw_calls: usize,
    /// Caller draws rejected by conservative frustum AABB tests.
    pub culled_draw_calls: usize,
    /// Actual `draw_indexed` commands encoded after culling and consecutive instancing.
    pub encoded_draw_calls: usize,
    pub triangles: usize,
    pub uploaded_meshes: usize,
    pub resident_geometry_bytes: usize,
}

struct UploadedMesh {
    source: Arc<MeshData>,
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
}

/// Opaque indexed renderer with vertex normals and one directional Blinn-Phong light.
/// Uses GpuContext/SurfaceTarget rather than a second device/surface implementation.
pub struct MeshRenderer {
    context: GpuContext,
    surface: Option<SurfaceTarget>,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    color: Option<wgpu::Texture>,
    depth: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    frame_uniforms: wgpu::Buffer,
    frame_bind: wgpu::BindGroup,
    instances: wgpu::Buffer,
    instance_bytes: Vec<u8>,
    cached: Vec<UploadedMesh>,
    has_frame: bool,
}

impl MeshRenderer {
    /// Creates a readable RGBA8 offscreen target. A missing adapter returns an error.
    pub fn headless(width: u32, height: u32) -> Result<Self, MeshError> {
        validate_extent(width, height, 4096)?;
        let context = GpuContext::headless().map_err(|error| MeshError::Gpu(error.to_string()))?;
        Self::new(context, None, width, height)
    }

    /// Uses the existing surface-compatible adapter selection.
    pub fn for_surface(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, MeshError> {
        validate_extent(width, height, 4096)?;
        let (context, surface) = GpuContext::for_surface(target, width, height)
            .map_err(|error| MeshError::Gpu(error.to_string()))?;
        Self::new(context, Some(surface), width, height)
    }

    fn new(
        context: GpuContext,
        surface: Option<SurfaceTarget>,
        width: u32,
        height: u32,
    ) -> Result<Self, MeshError> {
        let device = context.device();
        validate_extent(width, height, device.limits().max_texture_dimension_2d)?;
        let format = surface
            .as_ref()
            .map_or(wgpu::TextureFormat::Rgba8Unorm, SurfaceTarget::format);
        let scope = GpuScopes::new(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ExtremEngine lit indexed mesh shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("mesh.wgsl"))),
        });
        let vertices = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
            2 => Float32x3
        ];
        let instances_layout = wgpu::vertex_attr_array![
            3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x4, 7 => Float32x4, 8 => Float32x4
        ];
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: 36,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertices,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: INSTANCE_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instances_layout,
            }),
        ];
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ExtremEngine lit opaque indexed mesh pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        scope.check()?;
        let scope = GpuScopes::new(device);
        let frame_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ExtremEngine mesh camera and light"),
            size: FRAME_UNIFORM_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ExtremEngine mesh frame binding"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_uniforms.as_entire_binding(),
            }],
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ExtremEngine bounded mesh instances"),
            size: (MAX_FRAME_DRAWS * INSTANCE_STRIDE) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let color = surface
            .is_none()
            .then(|| target(device, width, height, format, true));
        let depth = target(device, width, height, DEPTH_FORMAT, false);
        scope.check()?;
        Ok(Self {
            context,
            surface,
            format,
            width,
            height,
            color,
            depth,
            pipeline,
            frame_uniforms,
            frame_bind,
            instances,
            instance_bytes: Vec::new(),
            cached: Vec::new(),
            has_frame: false,
        })
    }

    /// Describes the adapter actually selected, including backend and device type.
    pub fn adapter_description(&self) -> String {
        format!("{:?}", self.context.adapter().get_info())
    }

    /// Zero size suspends submission without configuring an invalid GPU surface.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<bool, MeshError> {
        if self.width == width && self.height == height {
            return Ok(false);
        }
        if width == 0 || height == 0 {
            self.width = width;
            self.height = height;
            self.has_frame = false;
            return Ok(true);
        }
        let device = self.context.device();
        validate_extent(width, height, device.limits().max_texture_dimension_2d)?;
        let scope = GpuScopes::new(device);
        let color = self
            .surface
            .is_none()
            .then(|| target(device, width, height, self.format, true));
        let depth = target(device, width, height, DEPTH_FORMAT, false);
        scope.check()?;
        if let Some(surface) = &mut self.surface {
            let scope = GpuScopes::new(device);
            surface.resize(device, width, height);
            if let Err(error) = scope.check() {
                self.has_frame = false;
                return Err(error);
            }
        }
        self.color = color;
        self.depth = depth;
        self.width = width;
        self.height = height;
        self.has_frame = false;
        Ok(true)
    }

    /// Compatibility entry point: ambient-only lighting reproduces the previous colors.
    pub fn render(
        &mut self,
        camera: Option<[f32; 16]>,
        draws: &[MeshDraw],
    ) -> Result<MeshFrameReport, MeshError> {
        self.render_lit(camera, [0.0, 0.0, 0.0], MeshLight::default(), draws)
    }

    /// Validates a whole lit frame before upload/encoding.
    ///
    /// `camera_position` is the world-space camera origin used for Blinn-Phong specular.
    /// Lambert-compatible frames may pass the origin when `specular_intensity` is zero.
    pub fn render_lit(
        &mut self,
        camera: Option<[f32; 16]>,
        camera_position: [f32; 3],
        light: MeshLight,
        draws: &[MeshDraw],
    ) -> Result<MeshFrameReport, MeshError> {
        let camera = match camera {
            Some(camera) => camera,
            None if draws.is_empty() => IDENTITY,
            None => return Err(MeshError::MissingCamera),
        };
        if !camera_position.iter().all(|value| value.is_finite()) {
            return Err(MeshError::InvalidMatrix);
        }
        validate_lit_frame(&camera, light, draws)?;
        let kept_indices = crate::frustum::retain_draws_in_frustum(&camera, draws)?;
        let culled_draw_calls = draws.len() - kept_indices.len();
        let kept: Vec<&MeshDraw> = kept_indices.iter().map(|&index| &draws[index]).collect();
        let mut seen = HashSet::new();
        let mut bytes = 0usize;
        for draw in draws {
            if seen.insert(Arc::as_ptr(&draw.mesh)) {
                bytes = bytes
                    .checked_add(draw.mesh.payload_bytes())
                    .ok_or(MeshError::Capacity)?;
                if seen.len() > MAX_RESIDENT_MESHES || bytes > MAX_GEOMETRY_BYTES {
                    return Err(MeshError::Capacity);
                }
            }
        }
        if self.width == 0 || self.height == 0 {
            return Ok(MeshFrameReport {
                surface_status: Some(SurfaceFrameStatus::Occluded),
                ..MeshFrameReport::default()
            });
        }
        let scope = GpuScopes::new(self.context.device());
        let acquired = self.surface.as_ref().map(|surface| {
            let mut frame = surface.acquire_frame();
            if matches!(
                frame,
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Lost | SurfaceFrameStatus::Outdated)
            ) {
                surface.reconfigure(self.context.device());
                frame = surface.acquire_frame();
            }
            frame
        });
        let (surface_texture, status) = match acquired {
            Some(SurfaceFrame::Renderable { texture, status }) => (Some(texture), Some(status)),
            Some(SurfaceFrame::Unavailable(status)) => {
                scope.check()?;
                return Ok(MeshFrameReport {
                    surface_status: Some(status),
                    ..MeshFrameReport::default()
                });
            }
            None => (None, None),
        };
        let texture = surface_texture
            .as_ref()
            .map(|frame| &frame.texture)
            .or(self.color.as_ref())
            .ok_or(MeshError::ReadbackUnavailable)?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.cached
            .retain(|entry| seen.contains(&Arc::as_ptr(&entry.source)));
        let mut uploaded = 0;
        let mut slots = Vec::with_capacity(kept.len());
        self.instance_bytes.clear();
        for draw in &kept {
            let slot = match self
                .cached
                .iter()
                .position(|entry| Arc::ptr_eq(&entry.source, &draw.mesh))
            {
                Some(slot) => slot,
                None => {
                    let entry = upload(
                        self.context.device(),
                        self.context.queue(),
                        Arc::clone(&draw.mesh),
                    );
                    self.cached.push(entry);
                    uploaded += 1;
                    self.cached.len() - 1
                }
            };
            slots.push(slot);
            for value in
                draw.model
                    .iter()
                    .chain(draw.color.iter())
                    .chain(&[draw.shininess, 0.0, 0.0, 0.0])
            {
                self.instance_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut frame_bytes = [0_u8; FRAME_UNIFORM_BYTES];
        for (chunk, value) in frame_bytes[..64].chunks_exact_mut(4).zip(camera) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        let light_values = [
            light.direction_to_light[0],
            light.direction_to_light[1],
            light.direction_to_light[2],
            light.intensity,
            light.color[0],
            light.color[1],
            light.color[2],
            light.ambient,
            camera_position[0],
            camera_position[1],
            camera_position[2],
            light.specular_intensity,
        ];
        for (chunk, value) in frame_bytes[64..112].chunks_exact_mut(4).zip(light_values) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        self.context
            .queue()
            .write_buffer(&self.frame_uniforms, 0, &frame_bytes);
        if !kept.is_empty() {
            self.context
                .queue()
                .write_buffer(&self.instances, 0, &self.instance_bytes);
        }
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("ExtremEngine lit mesh frame"),
                });
        let encoded_draw_calls;
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ExtremEngine lit opaque meshes"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
            pass.set_bind_group(0, &self.frame_bind, &[]);
            pass.set_vertex_buffer(1, self.instances.slice(..));
            encoded_draw_calls = visit_consecutive_batches(&slots, |slot, instances| {
                let mesh = &self.cached[slot];
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.source.indices().len() as u32, 0, instances);
            });
        }
        self.context.queue().submit(Some(encoder.finish()));
        if let Err(error) = scope.check() {
            self.cached.clear();
            self.has_frame = false;
            return Err(error);
        }
        if let Some(texture) = surface_texture {
            self.context.queue().present(texture);
            if status == Some(SurfaceFrameStatus::Suboptimal) {
                if let Some(surface) = &self.surface {
                    let scope = GpuScopes::new(self.context.device());
                    surface.reconfigure(self.context.device());
                    if let Err(error) = scope.check() {
                        self.has_frame = false;
                        return Err(error);
                    }
                }
            }
        }
        self.has_frame = true;
        Ok(MeshFrameReport {
            submitted: true,
            surface_status: status,
            draw_calls: draws.len(),
            culled_draw_calls,
            encoded_draw_calls,
            triangles: kept.iter().map(|draw| draw.mesh.indices().len() / 3).sum(),
            uploaded_meshes: uploaded,
            resident_geometry_bytes: bytes,
        })
    }

    /// Reads the last submitted offscreen RGBA8 image with padded GPU rows removed.
    pub fn read_rgba(&self) -> Result<Vec<u8>, MeshError> {
        if !self.has_frame || self.width == 0 || self.height == 0 {
            return Err(MeshError::ReadbackUnavailable);
        }
        let texture = self.color.as_ref().ok_or(MeshError::ReadbackUnavailable)?;
        let row = self.width.checked_mul(4).ok_or(MeshError::Capacity)?;
        let padded = row.div_ceil(256) * 256;
        let device = self.context.device();
        let scope = GpuScopes::new(device);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ExtremEngine mesh readback"),
            size: u64::from(padded) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ExtremEngine mesh readback copy"),
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.context.queue().submit(Some(encoder.finish()));
        scope.check()?;
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _delivery = sender.send(result);
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .map_err(|error| MeshError::Gpu(error.to_string()))?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| MeshError::Gpu(error.to_string()))?
            .map_err(|error| MeshError::Gpu(error.to_string()))?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| MeshError::Gpu(error.to_string()))?;
        let mut output = Vec::with_capacity(row as usize * self.height as usize);
        for bytes in mapped.chunks_exact(padded as usize) {
            output.extend_from_slice(&bytes[..row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(output)
    }
}

/// Visits maximal consecutive runs with the same uploaded mesh slot.
///
/// This deliberately does not reorder slots: opaque equal-depth behavior remains identical to
/// the caller's entity order, while adjacent instances of one geometry can share one GPU draw.
fn visit_consecutive_batches(slots: &[usize], mut visit: impl FnMut(usize, Range<u32>)) -> usize {
    let mut start = 0usize;
    let mut batches = 0usize;
    while start < slots.len() {
        let slot = slots[start];
        let mut end = start + 1;
        while end < slots.len() && slots[end] == slot {
            end += 1;
        }
        visit(slot, start as u32..end as u32);
        batches += 1;
        start = end;
    }
    batches
}

fn target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    readable: bool,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ExtremEngine bounded mesh target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if readable {
                wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, source: Arc<MeshData>) -> UploadedMesh {
    let mut vertices = Vec::with_capacity(source.vertices().len() * 36);
    for (vertex, normal) in source.vertices().iter().zip(source.normals()) {
        for value in vertex.position.iter().chain(&vertex.color).chain(normal) {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut indices = Vec::with_capacity(source.indices().len() * 4);
    for index in source.indices() {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    let vertex = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ExtremEngine lit mesh vertices"),
        size: vertices.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ExtremEngine mesh indices"),
        size: indices.len() as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex, 0, &vertices);
    queue.write_buffer(&index, 0, &indices);
    UploadedMesh {
        source,
        vertex,
        index,
    }
}

#[cfg(test)]
mod tests {
    use super::visit_consecutive_batches;

    fn collect(slots: &[usize]) -> (usize, Vec<(usize, std::ops::Range<u32>)>) {
        let mut batches = Vec::new();
        let count = visit_consecutive_batches(slots, |slot, range| batches.push((slot, range)));
        (count, batches)
    }

    #[test]
    fn empty_input_encodes_no_draws() {
        assert_eq!(collect(&[]), (0, Vec::new()));
    }

    #[test]
    fn repeated_geometry_collapses_to_one_instanced_draw() {
        assert_eq!(collect(&[4, 4, 4]), (1, vec![(4, 0..3)]));
    }

    #[test]
    fn separated_geometry_is_never_reordered_for_batching() {
        assert_eq!(
            collect(&[0, 1, 0, 0, 1]),
            (4, vec![(0, 0..1), (1, 1..2), (0, 2..4), (1, 4..5)])
        );
    }

    #[test]
    fn maximal_consecutive_runs_keep_original_instance_ranges() {
        assert_eq!(
            collect(&[2, 2, 1, 1, 1, 2]),
            (3, vec![(2, 0..2), (1, 2..5), (2, 5..6)])
        );
    }
}
