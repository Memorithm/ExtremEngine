//! Opt-in world-mesh backend using the existing GPU context and surface authority.
use crate::{Engine, EngineConfig};
use extrem_ecs::{Entity, World};
use extrem_gpu::{MAX_FRAME_DRAWS, MeshDraw, MeshRenderer};
use extrem_math::Transform;
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand};
use extrem_scene::{GlobalTransform, Visibility};
use std::sync::Arc;

pub use extrem_gpu::{MeshData, MeshError, MeshFrameReport, MeshVertex};

/// ECS component for an opaque, unlit, vertex-colored mesh instance.
/// Geometry is immutable and can be shared. The current local/global Transform is
/// read after App stages on every frame; Visibility(false) hides this instance.
#[derive(Clone, Debug)]
pub struct MeshInstance {
    pub geometry: Arc<MeshData>,
    pub color: [f32; 4],
}

/// CPU extraction observations, independent from actual GPU submission success.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshExtractionStats {
    pub visible: usize,
    pub hidden: usize,
    pub missing_transform: usize,
}

/// Bounded reusable mesh extraction. No GPU or window is needed to test this path.
#[derive(Debug, Default)]
pub struct MeshExtractor {
    items: Vec<(Entity, MeshDraw)>,
    draws: Vec<MeshDraw>,
}

impl MeshExtractor {
    /// Reads all MeshInstance entities, validates transforms/materials and sorts IDs.
    /// On error the submitted draw slice is empty, never a partial visible scene.
    pub fn extract(&mut self, world: &World) -> Result<MeshExtractionStats, MeshError> {
        self.items.clear();
        self.draws.clear();
        let mut stats = MeshExtractionStats::default();
        for (entity, instance) in world.iter::<MeshInstance>() {
            if world.get::<Visibility>(entity).is_some_and(|visibility| !visibility.0) {
                stats.hidden += 1;
                continue;
            }
            let transform = world.get::<GlobalTransform>(entity).map(|global| global.0)
                .or_else(|| world.get::<Transform>(entity).copied());
            let Some(transform) = transform else {
                stats.missing_transform += 1;
                continue;
            };
            if !transform.is_valid() {
                self.items.clear();
                return Err(MeshError::InvalidMatrix);
            }
            if self.items.len() == MAX_FRAME_DRAWS {
                self.items.clear();
                return Err(MeshError::Capacity);
            }
            let draw = MeshDraw {
                mesh: Arc::clone(&instance.geometry),
                model: transform.to_mat4().data,
                color: instance.color,
            };
            if let Err(error) = draw.validate() {
                self.items.clear();
                return Err(error);
            }
            self.items.push((entity, draw));
        }
        self.items.sort_unstable_by_key(|(entity, _)| *entity);
        stats.visible = self.items.len();
        self.draws.extend(self.items.drain(..).map(|(_, draw)| draw));
        Ok(stats)
    }

    pub fn draws(&self) -> &[MeshDraw] {
        &self.draws
    }
}

/// Real opaque indexed rendering. The legacy WgpuRenderer remains a validation
/// triangle presenter; this backend deliberately does not draw a fallback triangle.
/// Use Engine::with_mesh_renderer so mesh extraction is attached to the frame loop.
pub struct WgpuMeshRenderer {
    gpu: MeshRenderer,
    extractor: MeshExtractor,
    camera: Option<[f32; 16]>,
    extraction_error: Option<MeshError>,
    last_extraction: MeshExtractionStats,
    last_result: Option<Result<MeshFrameReport, MeshError>>,
    submitted_commands: usize,
}

impl WgpuMeshRenderer {
    pub fn new(gpu: MeshRenderer) -> Self {
        Self {
            gpu,
            extractor: MeshExtractor::default(),
            camera: None,
            extraction_error: None,
            last_extraction: MeshExtractionStats::default(),
            last_result: None,
            submitted_commands: 0,
        }
    }

    pub fn gpu(&self) -> &MeshRenderer {
        &self.gpu
    }

    pub fn gpu_mut(&mut self) -> &mut MeshRenderer {
        &mut self.gpu
    }

    /// GPU failures/surface-unavailability are separate from tick's RenderGraphError.
    /// A successful tick alone does not prove that a GPU frame was submitted.
    pub fn last_mesh_result(&self) -> Option<&Result<MeshFrameReport, MeshError>> {
        self.last_result.as_ref()
    }

    pub fn last_mesh_extraction(&self) -> MeshExtractionStats {
        self.last_extraction
    }

    fn extract(world: &World, renderer: &mut Self) {
        match renderer.extractor.extract(world) {
            Ok(stats) => renderer.last_extraction = stats,
            Err(error) => renderer.extraction_error = Some(error),
        }
    }
}

impl RenderBackend for WgpuMeshRenderer {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.camera = None;
        self.extraction_error = None;
        self.last_extraction = MeshExtractionStats::default();
        self.submitted_commands = 0;
    }

    fn submit(&mut self, command: RenderCommand) {
        self.submitted_commands += 1;
        if let RenderCommand::SetCamera { view_projection, .. } = command {
            self.camera = Some(view_projection.data);
        }
    }

    fn end_frame(&mut self) -> FrameStats {
        self.last_result = Some(match self.extraction_error.take() {
            Some(error) => Err(error),
            None => self.gpu.render(self.camera, self.extractor.draws()),
        });
        FrameStats {
            // Preserve the legacy diagnostic's camera/translation command meaning.
            // Mesh draw calls and triangles have their own MeshFrameReport counters.
            submitted_commands: self.submitted_commands,
            drawn_pixels: 0,
        }
    }
}

impl Engine<WgpuMeshRenderer> {
    /// Attaches actual mesh extraction to the normal validated Engine::tick path.
    /// Graph errors still reject before App/backend work; GPU errors are available
    /// from renderer().last_mesh_result() and do not roll back simulation.
    pub fn with_mesh_renderer(renderer: WgpuMeshRenderer, config: EngineConfig) -> Self {
        let mut engine = Self::with_renderer(renderer, config);
        engine.set_backend_extractor(WgpuMeshRenderer::extract);
        engine
    }
}
