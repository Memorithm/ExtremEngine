//! Actual ECS cubes using the same mesh pipeline in a window or offscreen.
use extrem_engine::{
    Camera, Engine, EngineConfig, MeshData, MeshInstance, MeshVertex, Stage,
    WgpuMeshRenderer, WindowConfig, WindowHost,
};
use extrem_gpu::MeshRenderer;
use extrem_math::{Quat, Transform, Vec3};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

type MeshEngine = Engine<WgpuMeshRenderer>;

fn cube() -> Result<Arc<MeshData>, Box<dyn Error>> {
    let faces = [
        [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]],
        [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]],
        [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]],
        [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]],
        [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]],
        [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]],
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (face, positions) in faces.into_iter().enumerate() {
        let base = vertices.len() as u32;
        // Deliberate vertex coloring, not a physical lighting claim.
        let shade = [0.85, 0.65, 0.55, 0.75, 1.0, 0.45][face];
        for position in positions {
            vertices.push(MeshVertex { position, color: [shade; 3] });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok(MeshData::new(vertices, indices)?)
}

fn scene(gpu: MeshRenderer, aspect: f32) -> Result<MeshEngine, Box<dyn Error>> {
    println!("adapter={}", gpu.adapter_description());
    let mut engine = Engine::with_mesh_renderer(WgpuMeshRenderer::new(gpu), EngineConfig {
        viewport_aspect: aspect,
        ..EngineConfig::default()
    });
    let mesh = cube()?;
    for (index, color) in [[0.10, 0.50, 0.55, 1.0], [0.85, 0.27, 0.08, 1.0], [0.30, 0.36, 0.42, 1.0]].into_iter().enumerate() {
        let entity = engine.world_mut().try_spawn(Transform {
            translation: Vec3::new((index as f32 - 1.0) * 1.35, 0.0, 0.0),
            rotation: Quat::from_euler(0.2, 0.4 + index as f32 * 0.25, 0.0),
            scale: Vec3::ONE,
        })?;
        engine.world_mut().insert(entity, MeshInstance { geometry: Arc::clone(&mesh), color })?;
    }
    let camera = engine.world_mut().try_spawn(Transform {
        translation: Vec3::new(0.0, 1.4, 5.0),
        rotation: Quat::from_euler(-0.20, 0.0, 0.0),
        scale: Vec3::ONE,
    })?;
    engine.world_mut().insert(camera, Camera::default())?;
    engine.app_mut().add_systems(Stage::Update, |world, time| {
        let entities: Vec<_> = world.iter::<MeshInstance>().map(|(id, _)| id).collect();
        for entity in entities {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.rotation = Quat::from_euler(0.2, time.elapsed_seconds as f32 * 0.4 + transform.translation.x * 0.2, 0.0);
            }
        }
    });
    Ok(engine)
}

fn verify_frame(engine: &MeshEngine) -> Result<(), Box<dyn Error>> {
    let report = engine.renderer().last_mesh_result().ok_or("missing mesh report")?.as_ref()?;
    if report.submitted {
        assert_eq!((report.draw_calls, report.triangles), (3, 36));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--headless") {
        let path = args.get(1).map_or("mesh-cubes.ppm", String::as_str);
        let mut engine = scene(MeshRenderer::headless(640, 480)?, 640.0 / 480.0)?;
        engine.tick(0.25)?;
        verify_frame(&engine)?;
        let pixels = engine.renderer().gpu().read_rgba()?;
        let changed = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [255; 3]).count();
        assert!(changed > 1000, "the projected meshes must occupy visible pixels");
        let mut ppm = b"P6\n640 480\n255\n".to_vec();
        for pixel in pixels.chunks_exact(4) { ppm.extend_from_slice(&pixel[..3]); }
        std::fs::write(path, ppm)?;
        println!("mesh_scene_passed: 3 instances, 36 triangles, nonwhite_pixels={changed}");
        return Ok(());
    }
    let mut engine: Option<MeshEngine> = None;
    let mut failed = false;
    let mut previous = Instant::now();
    WindowHost::run_with_input(WindowConfig {
        title: "ExtremEngine — indexed world meshes".into(),
        width: 960,
        height: 640,
    }, move |window, input| {
        if failed { return; }
        let result = (|| -> Result<(), Box<dyn Error>> {
            let size = window.inner_size();
            if engine.is_none() {
                if size.width == 0 || size.height == 0 { return Ok(()); }
                let gpu = MeshRenderer::for_surface(Arc::clone(window), size.width, size.height)?;
                engine = Some(scene(gpu, size.width as f32 / size.height as f32)?);
            }
            let engine = engine.as_mut().ok_or("missing initialized engine")?;
            engine.renderer_mut().gpu_mut().resize(size.width, size.height)?;
            if size.width == 0 || size.height == 0 { previous = Instant::now(); return Ok(()); }
            engine.set_viewport_aspect(size.width as f32 / size.height as f32);
            engine.set_input_snapshot(input);
            let now = Instant::now();
            let delta = now.duration_since(previous).as_secs_f32().min(0.1);
            previous = now;
            engine.tick(delta)?;
            verify_frame(engine)
        })();
        if let Err(error) = result {
            eprintln!("Mesh demo stopped: {error}. Close the window to exit.");
            window.set_title("ExtremEngine — rendering error (see terminal)");
            failed = true;
        }
    })?;
    Ok(())
}
