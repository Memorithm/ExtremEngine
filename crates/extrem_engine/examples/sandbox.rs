use extrem_engine::{BoxCollider, Camera, Engine, Input, KeyCode, RigidBody, Scene, Stage};
use extrem_math::{Transform, Vec3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    let mut scene = Scene::new("sandbox");
    let entity = scene.spawn_entity(
        engine.world_mut(),
        "falling-cube",
        Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
    )?;
    engine.world_mut().insert(entity, RigidBody::default())?;
    engine.world_mut().insert(entity, BoxCollider::default())?;

    let camera = engine.world_mut().try_spawn_empty()?;
    engine.world_mut().insert(
        camera,
        Transform::from_translation(Vec3::new(0.0, 2.0, 5.0)),
    )?;
    engine.world_mut().insert(camera, Camera::default())?;

    let mut input = Input::default();
    input.keys.press(KeyCode::W);
    engine.world_mut().insert_resource(input);

    engine
        .app_mut()
        .add_systems(Stage::Update, move |world, time| {
            let pressed = world
                .get_resource::<Input>()
                .is_some_and(|input| input.keys.just_pressed(KeyCode::W));
            if pressed {
                if let Some(transform) = world.get_mut::<Transform>(entity) {
                    transform.translation.x += time.delta_seconds;
                }
            }
        });

    let delta_seconds = engine.config().target_delta_seconds;
    for _ in 0..3 {
        let report = engine.tick(delta_seconds)?;
        let position = engine
            .world()
            .get::<Transform>(entity)
            .ok_or("sandbox entity has no transform")?
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
        println!("render_passes={:?}", engine.last_render_passes());
    }
    Ok(())
}
