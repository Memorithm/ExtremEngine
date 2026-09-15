//! Required WGPU pixel qualification; missing adapters are errors, never skipped success.
use extrem_engine::{
    Camera, Engine, EngineConfig, MeshData, MeshError, MeshInstance, MeshVertex,
    Projection, Visibility, WgpuMeshRenderer,
};
use extrem_gpu::{MeshDraw, MeshRenderer};
use extrem_math::{Mat4, Transform, Vec3};
use std::sync::Arc;

fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    &image[(y * width + x) * 4..(y * width + x) * 4 + 4]
}

fn ppm(path: &str, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in rgba.chunks_exact(4) { data.extend_from_slice(&pixel[..3]); }
    std::fs::write(path, data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).unwrap_or_else(|| "mesh-depth.ppm".into());
    let geometry = MeshData::new(vec![
        MeshVertex { position: [-0.5, -0.5, 0.0], color: [1.0; 3] },
        MeshVertex { position: [0.5, -0.5, 0.0], color: [1.0; 3] },
        MeshVertex { position: [0.5, 0.5, 0.0], color: [1.0; 3] },
        MeshVertex { position: [-0.5, 0.5, 0.0], color: [1.0; 3] },
    ], vec![0, 1, 2, 0, 2, 3])?;
    let gpu = MeshRenderer::headless(65, 49)?;
    println!("adapter={}", gpu.adapter_description());
    let mut engine = Engine::with_mesh_renderer(WgpuMeshRenderer::new(gpu), EngineConfig {
        viewport_aspect: 65.0 / 49.0,
        ..EngineConfig::default()
    });
    let near = engine.world_mut().try_spawn(Transform::from_translation(Vec3::new(-0.15, 0.0, 0.7)))?;
    engine.world_mut().insert(near, MeshInstance { geometry: Arc::clone(&geometry), color: [1.0, 0.0, 0.0, 1.0] })?;
    let far = engine.world_mut().try_spawn(Transform::from_translation(Vec3::new(0.15, 0.0, 0.2)))?;
    engine.world_mut().insert(far, MeshInstance { geometry: Arc::clone(&geometry), color: [0.0, 0.0, 1.0, 1.0] })?;
    engine.tick(0.0)?;
    assert_eq!(engine.renderer().last_mesh_result(), Some(&Err(MeshError::MissingCamera)));
    let camera = engine.world_mut().try_spawn(Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)))?;
    engine.world_mut().insert(camera, Camera {
        active: true,
        projection: Projection::Orthographic { width: 2.0, height: 2.0, near: 0.0, far: 1.0 },
    })?;
    engine.tick(0.0)?;
    let first = engine.renderer().last_mesh_result().ok_or("no mesh result")?.clone()?;
    assert!(first.submitted);
    assert_eq!((first.draw_calls, first.triangles, first.uploaded_meshes), (2, 4, 1));
    let image = engine.renderer().gpu().read_rgba()?;
    assert_eq!(image.len(), 65 * 49 * 4);
    assert_eq!(pixel(&image, 65, 32, 24), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 65, 48, 24), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 65, 0, 0), [255; 4]);
    ppm(&output, 65, 49, &image)?;
    engine.tick(0.0)?;
    assert_eq!(engine.renderer().last_mesh_result().ok_or("no result")?.as_ref()?.uploaded_meshes, 0);
    assert_eq!(engine.renderer().gpu().read_rgba()?, image);
    engine.world_mut().insert(near, Visibility(false))?;
    engine.tick(0.0)?;
    assert_eq!(pixel(&engine.renderer().gpu().read_rgba()?, 65, 32, 24), [0, 0, 255, 255]);
    engine.world_mut().insert(near, Visibility(true))?;
    engine.world_mut().get_mut::<Transform>(camera).ok_or("camera transform")?.translation.x = 0.5;
    engine.tick(0.0)?;
    assert_eq!(pixel(&engine.renderer().gpu().read_rgba()?, 65, 32, 24), [0, 0, 255, 255]);
    engine.world_mut().get_mut::<Transform>(camera).ok_or("camera transform")?.translation.x = 0.0;
    engine.renderer_mut().gpu_mut().resize(0, 0)?;
    engine.tick(0.0)?;
    assert!(!engine.renderer().last_mesh_result().ok_or("no result")?.as_ref()?.submitted);
    assert!(engine.renderer().gpu().read_rgba().is_err());
    engine.renderer_mut().gpu_mut().resize(67, 51)?;
    engine.tick(0.0)?;
    assert_eq!(pixel(&engine.renderer().gpu().read_rgba()?, 67, 33, 25), [255, 0, 0, 255]);
    // Directly reverse opaque draw order on the SAME pipeline and compare all pixels.
    let draws = [
        MeshDraw { mesh: Arc::clone(&geometry), model: Mat4::translation(Vec3::new(-0.15, 0.0, 0.2)).data, color: [1.0, 0.0, 0.0, 1.0] },
        MeshDraw { mesh: geometry, model: Mat4::translation(Vec3::new(0.15, 0.0, 0.7)).data, color: [0.0, 0.0, 1.0, 1.0] },
    ];
    let gpu = engine.renderer_mut().gpu_mut();
    gpu.render(Some(Mat4::IDENTITY.data), &draws)?;
    let forward = gpu.read_rgba()?;
    gpu.render(Some(Mat4::IDENTITY.data), &[draws[1].clone(), draws[0].clone()])?;
    assert_eq!(gpu.read_rgba()?, forward);
    let mut invalid = draws[0].clone();
    invalid.color[3] = 0.5;
    assert_eq!(gpu.render(Some(Mat4::IDENTITY.data), &[invalid]), Err(MeshError::InvalidColor));
    assert_eq!(gpu.read_rgba()?, forward);
    println!("PASS: indexed geometry, shared upload, per-instance material/model, depth order, camera, visibility, padded readback, suspend/resume, rejection");
    Ok(())
}
