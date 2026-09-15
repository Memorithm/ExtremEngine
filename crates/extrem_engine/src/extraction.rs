//! Backend-neutral extraction of the existing camera/translation command stream.
use extrem_ecs::{Entity, World};
use extrem_math::{Transform, Vec3};
use extrem_render::{RenderBackend, RenderCommand};
use extrem_scene::{Camera, GlobalTransform};

/// Work observed during one extraction, not elapsed time or allocator telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderExtractionStats {
    /// Active cameras that have a global or local transform.
    pub eligible_cameras: usize,
    /// The lowest eligible entity ID, including its generation.
    pub selected_camera: Option<Entity>,
    /// Actual calls to camera view-projection construction (zero or one).
    pub camera_matrices: usize,
    /// Translation commands, excluding the optional camera command.
    pub transform_commands: usize,
    /// Retained capacity in `(Entity, Vec3)` entries, not bytes or allocations.
    pub scratch_capacity: usize,
}

/// Reusable scratch for the current renderer's camera/translation contract.
///
/// Each call rereads the World. Global transforms take precedence over locals;
/// global-only entities remain visible to extraction. Commands are submitted in
/// ascending entity order after the selected camera, exactly as in the old path.
/// This does not add visibility filtering, mesh rendering or numeric validation.
///
/// # Examples
///
/// ```
/// use extrem_ecs::World;
/// use extrem_engine::RenderExtractor;
/// use extrem_math::Transform;
/// use extrem_render::{FrameInfo, NullRenderer, RenderBackend};
///
/// let mut world = World::new();
/// world.try_spawn(Transform::IDENTITY)?;
/// let mut extractor = RenderExtractor::default();
/// let mut renderer = NullRenderer::default();
/// renderer.begin_frame(FrameInfo { index: 1, delta_seconds: 0.0 });
/// let stats = extractor.submit(&world, 1.0, &mut renderer);
/// assert_eq!(stats.transform_commands, 1);
/// assert_eq!(renderer.end_frame().submitted_commands, 1);
/// # Ok::<(), extrem_ecs::WorldError>(())
/// ```
#[derive(Debug, Default)]
pub struct RenderExtractor {
    translations: Vec<(Entity, Vec3)>,
}

impl RenderExtractor {
    /// Extracts and submits without beginning/ending a backend frame.
    ///
    /// Selects a camera before constructing its matrix, avoiding computations for
    /// discarded candidates. Sorting uses unique Entity keys, so an unstable sort
    /// preserves the old observable order without a stable-sort scratch buffer.
    /// Backend panics, allocation failure and invalid numerics are not intercepted.
    pub fn submit<R: RenderBackend>(
        &mut self,
        world: &World,
        aspect: f32,
        renderer: &mut R,
    ) -> RenderExtractionStats {
        self.translations.clear();
        let mut stats = RenderExtractionStats::default();
        let mut selected: Option<(Entity, Camera, Transform)> = None;
        for (entity, camera) in world.iter::<Camera>() {
            if !camera.active {
                continue;
            }
            let transform = world
                .get::<GlobalTransform>(entity)
                .map(|global| global.0)
                .or_else(|| world.get::<Transform>(entity).copied());
            let Some(transform) = transform else {
                continue;
            };
            stats.eligible_cameras += 1;
            if selected.as_ref().is_none_or(|(id, _, _)| entity < *id) {
                selected = Some((entity, *camera, transform));
            }
        }
        if let Some((entity, camera, transform)) = selected {
            stats.selected_camera = Some(entity);
            let view_projection = camera.view_projection(transform, aspect);
            stats.camera_matrices += 1;
            renderer.submit(RenderCommand::SetCamera {
                entity,
                view_projection,
            });
        }

        self.translations.extend(
            world
                .iter::<GlobalTransform>()
                .map(|(entity, global)| (entity, global.0.translation)),
        );
        self.translations.extend(
            world
                .iter::<Transform>()
                .filter(|(entity, _)| world.get::<GlobalTransform>(*entity).is_none())
                .map(|(entity, local)| (entity, local.translation)),
        );
        // Each component map has unique Entity keys, and the two sets are disjoint.
        self.translations
            .sort_unstable_by_key(|(entity, _)| *entity);
        stats.transform_commands = self.translations.len();
        for &(entity, translation) in &self.translations {
            renderer.submit(RenderCommand::Transform {
                entity,
                translation,
            });
        }
        stats.scratch_capacity = self.translations.capacity();
        stats
    }

    /// Retained entry capacity; this is not an allocation-count measurement.
    pub fn scratch_capacity(&self) -> usize {
        self.translations.capacity()
    }

    /// Drops retained scratch, without promising an operating-system RSS reduction.
    pub fn release_memory(&mut self) {
        self.translations = Vec::new();
    }
}
