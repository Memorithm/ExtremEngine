use std::collections::HashSet;

use extrem_app::{App, MinimalPlugins, UpdateReport};
use extrem_ecs::World;
use extrem_math::Transform;
use extrem_render::{FrameInfo, FrameStats, NullRenderer, RenderBackend, RenderCommand};
use extrem_scene::propagate_transforms;

pub use extrem_app::{Stage, Time};
pub use extrem_ecs::{Entity, WorldError};
pub use extrem_scene::{Children, GlobalTransform, Name, Parent, Scene, Velocity, Visibility};

/// Configuration for the high-level engine facade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineConfig {
    pub target_delta_seconds: f32,
    pub fixed_delta_seconds: f32,
    pub max_fixed_steps_per_frame: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            target_delta_seconds: 1.0 / 60.0,
            fixed_delta_seconds: 1.0 / 60.0,
            max_fixed_steps_per_frame: 8,
        }
    }
}

/// The engine owns the application lifecycle and a replaceable renderer.
pub struct Engine<R: RenderBackend = NullRenderer> {
    app: App,
    renderer: R,
    config: EngineConfig,
    last_frame_stats: FrameStats,
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

impl<R: RenderBackend> Engine<R> {
    pub fn with_renderer(renderer: R, config: EngineConfig) -> Self {
        let mut app = App::new();
        app.add_plugin(MinimalPlugins);
        app.set_fixed_timestep(config.fixed_delta_seconds)
            .set_max_fixed_steps_per_frame(config.max_fixed_steps_per_frame)
            .add_systems(extrem_app::Stage::PostUpdate, |world, _| {
                propagate_transforms(world)
            });
        Self {
            app,
            renderer,
            config,
            last_frame_stats: FrameStats::default(),
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

    pub fn tick(&mut self, delta_seconds: f32) -> UpdateReport {
        let report = self.app.update(delta_seconds);
        self.renderer.begin_frame(FrameInfo {
            index: report.frame,
            delta_seconds: report.delta_seconds,
        });
        let global_entities: HashSet<_> = self
            .world()
            .iter::<GlobalTransform>()
            .map(|(entity, _)| entity)
            .collect();
        let mut commands: Vec<_> = self
            .world()
            .iter::<GlobalTransform>()
            .map(|(entity, transform)| RenderCommand::Transform {
                entity,
                translation: transform.0.translation,
            })
            .collect();
        commands.extend(
            self.world()
                .iter::<Transform>()
                .filter(|(entity, _)| !global_entities.contains(entity))
                .map(|(entity, transform)| RenderCommand::Transform {
                    entity,
                    translation: transform.translation,
                }),
        );
        for command in commands {
            self.renderer.submit(command);
        }
        self.last_frame_stats = self.renderer.end_frame();
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
}

#[cfg(test)]
mod tests {
    use super::{Engine, Stage};
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
    }
}
