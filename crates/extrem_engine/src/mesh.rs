//! Opt-in world-mesh backend using the existing GPU context and surface authority.
use crate::{Engine, EngineConfig};
use extrem_ecs::{Entity, World};
use extrem_gpu::{MAX_FRAME_DRAWS, MeshDraw, MeshLight, MeshRenderer};
use extrem_math::{Transform, Vec3};
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand};
use extrem_scene::{GlobalTransform, Visibility};
use std::sync::Arc;

pub use extrem_gpu::{MeshData, MeshError, MeshFrameReport, MeshVertex};

/// ECS component for an opaque vertex-colored mesh instance.
#[derive(Clone, Debug)]
pub struct MeshInstance {
    pub geometry: Arc<MeshData>,
    pub color: [f32; 4],
    /// Blinn-Phong shininess. Zero keeps Lambert-only shading for this instance.
    pub shininess: f32,
}

/// One world-space directional light. The lowest active Entity ID wins deterministically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    pub active: bool,
    pub direction_to_light: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
    pub ambient: f32,
    /// Specular intensity. Zero preserves Lambert-only lighting.
    pub specular_intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            active: true,
            direction_to_light: Vec3::new(0.4, 0.8, 0.5),
            color: [1.0; 3],
            intensity: 0.85,
            ambient: 0.15,
            specular_intensity: 0.0,
        }
    }
}

impl DirectionalLight {
    fn gpu(self) -> MeshLight {
        MeshLight {
            direction_to_light: [
                self.direction_to_light.x,
                self.direction_to_light.y,
                self.direction_to_light.z,
            ],
            color: self.color,
            intensity: self.intensity,
            ambient: self.ambient,
            specular_intensity: self.specular_intensity,
        }
    }
}

/// CPU extraction observations, independent from actual GPU submission success.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshExtractionStats {
    pub visible: usize,
    pub hidden: usize,
    pub missing_transform: usize,
    pub eligible_lights: usize,
    pub selected_light: Option<Entity>,
}

/// Bounded reusable mesh extraction. No GPU or window is needed to test mesh collection.
#[derive(Debug, Default)]
pub struct MeshExtractor {
    items: Vec<(Entity, MeshDraw)>,
    draws: Vec<MeshDraw>,
}

impl MeshExtractor {
    /// Reads all MeshInstance entities, validates transforms/materials and sorts IDs.
    pub fn extract(&mut self, world: &World) -> Result<MeshExtractionStats, MeshError> {
        self.items.clear();
        self.draws.clear();
        let mut stats = MeshExtractionStats::default();
        for (entity, instance) in world.iter::<MeshInstance>() {
            if world
                .get::<Visibility>(entity)
                .is_some_and(|visibility| !visibility.0)
            {
                stats.hidden += 1;
                continue;
            }
            let transform = world
                .get::<GlobalTransform>(entity)
                .map(|global| global.0)
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
                shininess: instance.shininess,
            };
            if let Err(error) = draw.validate() {
                self.items.clear();
                return Err(error);
            }
            self.items.push((entity, draw));
        }
        self.items.sort_unstable_by_key(|(entity, _)| *entity);
        stats.visible = self.items.len();
        self.draws
            .extend(self.items.drain(..).map(|(_, draw)| draw));
        Ok(stats)
    }

    pub fn draws(&self) -> &[MeshDraw] {
        &self.draws
    }

    fn clear_draws(&mut self) {
        self.items.clear();
        self.draws.clear();
    }
}

/// Real opaque indexed rendering with one selected directional light.
pub struct WgpuMeshRenderer {
    gpu: MeshRenderer,
    extractor: MeshExtractor,
    camera: Option<[f32; 16]>,
    camera_position: [f32; 3],
    light: MeshLight,
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
            camera_position: [0.0, 0.0, 0.0],
            light: MeshLight::default(),
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

    pub fn last_mesh_result(&self) -> Option<&Result<MeshFrameReport, MeshError>> {
        self.last_result.as_ref()
    }

    pub fn last_mesh_extraction(&self) -> MeshExtractionStats {
        self.last_extraction
    }

    fn extract(world: &World, renderer: &mut Self) {
        let mut stats = match renderer.extractor.extract(world) {
            Ok(stats) => stats,
            Err(error) => {
                renderer.extraction_error = Some(error);
                return;
            }
        };
        renderer.light = MeshLight::default();
        let mut selected: Option<(Entity, DirectionalLight)> = None;
        for (entity, light) in world.iter::<DirectionalLight>() {
            if !light.active {
                continue;
            }
            stats.eligible_lights += 1;
            if selected
                .as_ref()
                .is_none_or(|(selected_entity, _)| entity < *selected_entity)
            {
                selected = Some((entity, *light));
            }
        }
        if let Some((entity, light)) = selected {
            let light = light.gpu();
            if let Err(error) = light.validate() {
                renderer.extractor.clear_draws();
                renderer.last_extraction = stats;
                renderer.extraction_error = Some(error);
                return;
            }
            renderer.light = light;
            stats.selected_light = Some(entity);
        }
        renderer.last_extraction = stats;
        renderer.extraction_error = None;
    }
}

impl RenderBackend for WgpuMeshRenderer {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.camera = None;
        self.camera_position = [0.0, 0.0, 0.0];
        self.light = MeshLight::default();
        self.extraction_error = Some(MeshError::MissingExtraction);
        self.last_extraction = MeshExtractionStats::default();
        self.submitted_commands = 0;
    }

    fn submit(&mut self, command: RenderCommand) {
        self.submitted_commands += 1;
        if let RenderCommand::SetCamera {
            view_projection,
            world_position,
            ..
        } = command
        {
            self.camera = Some(view_projection.data);
            self.camera_position = [world_position.x, world_position.y, world_position.z];
        }
    }

    fn end_frame(&mut self) -> FrameStats {
        self.last_result = Some(match self.extraction_error.take() {
            Some(error) => Err(error),
            None => self.gpu.render_lit(
                self.camera,
                self.camera_position,
                self.light,
                self.extractor.draws(),
            ),
        });
        FrameStats {
            submitted_commands: self.submitted_commands,
            drawn_pixels: 0,
        }
    }
}

impl Engine<WgpuMeshRenderer> {
    /// Attaches actual mesh/light extraction to the normal validated Engine::tick path.
    pub fn with_mesh_renderer(renderer: WgpuMeshRenderer, config: EngineConfig) -> Self {
        let mut engine = Self::with_renderer(renderer, config);
        engine.set_backend_extractor(WgpuMeshRenderer::extract);
        engine
    }
}
