use extrem_engine::{Engine, Stage};
use extrem_math::{Transform, Vec3};
use extrem_scene::Velocity;

fn main() {
    let mut engine = Engine::new();
    let entity = engine.world_mut().spawn_empty();
    engine
        .world_mut()
        .insert(
            entity,
            Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
        )
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

    let delta_seconds = engine.config().target_delta_seconds;
    for _ in 0..3 {
        let report = engine.tick(delta_seconds);
        let position = engine
            .world()
            .get::<Transform>(entity)
            .expect("transform")
            .translation;
        println!(
            "frame={} elapsed={:.3}s position=({:.3}, {:.3}, {:.3}) render_commands={}",
            report.frame,
            report.elapsed_seconds,
            position.x,
            position.y,
            position.z,
            engine.last_frame_stats().submitted_commands,
        );
    }
}
