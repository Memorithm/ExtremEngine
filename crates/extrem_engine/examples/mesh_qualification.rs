//! Required WGPU pixel qualification; missing adapters are errors, never skipped success.
use extrem_engine::{
    Camera, DirectionalLight, Engine, EngineConfig, MeshData, MeshError, MeshInstance, MeshVertex,
    Projection, Visibility, WgpuMeshRenderer,
};
use extrem_gpu::{MeshDraw, MeshLight, MeshRenderer, shade_lambert};
use extrem_math::{Mat4, Quat, Transform, Vec3};
use std::sync::Arc;

fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    &image[(y * width + x) * 4..(y * width + x) * 4 + 4]
}

fn ppm(path: &str, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in rgba.chunks_exact(4) {
        data.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(path, data)
}

fn unorm8(rgb: [f32; 3]) -> [u8; 4] {
    [
        (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

fn assert_pixel_near(actual: &[u8], expected: [u8; 4]) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "pixel channel {actual} != {expected}"
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "mesh-depth.ppm".into());
    let geometry = MeshData::new(
        vec![
            MeshVertex {
                position: [-0.5, -0.5, 0.0],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, -0.5, 0.0],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [0.5, 0.5, 0.0],
                color: [1.0; 3],
            },
            MeshVertex {
                position: [-0.5, 0.5, 0.0],
                color: [1.0; 3],
            },
        ],
        vec![0, 1, 2, 0, 2, 3],
    )?;
    assert!(geometry.normals().iter().all(|normal| normal[2] > 0.99));
    let gpu = MeshRenderer::headless(65, 49)?;
    println!("adapter={}", gpu.adapter_description());
    let mut engine = Engine::with_mesh_renderer(
        WgpuMeshRenderer::new(gpu),
        EngineConfig {
            viewport_aspect: 65.0 / 49.0,
            ..EngineConfig::default()
        },
    );
    let near = engine
        .world_mut()
        .try_spawn(Transform::from_translation(Vec3::new(-0.15, 0.0, 0.7)))?;
    engine.world_mut().insert(
        near,
        MeshInstance {
            geometry: Arc::clone(&geometry),
            color: [1.0, 0.0, 0.0, 1.0],
            shininess: 0.0,
        },
    )?;
    let far = engine
        .world_mut()
        .try_spawn(Transform::from_translation(Vec3::new(0.15, 0.0, 0.2)))?;
    engine.world_mut().insert(
        far,
        MeshInstance {
            geometry: Arc::clone(&geometry),
            color: [0.0, 0.0, 1.0, 1.0],
            shininess: 0.0,
        },
    )?;
    engine.tick(0.0)?;
    assert_eq!(
        engine.renderer().last_mesh_result(),
        Some(&Err(MeshError::MissingCamera))
    );
    let camera = engine
        .world_mut()
        .try_spawn(Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)))?;
    engine.world_mut().insert(
        camera,
        Camera {
            active: true,
            projection: Projection::Orthographic {
                width: 2.0,
                height: 2.0,
                near: 0.0,
                far: 1.0,
            },
        },
    )?;
    let light_gpu = MeshLight {
        direction_to_light: [0.0, 0.0, 1.0],
        color: [1.0; 3],
        intensity: 0.6,
        ambient: 0.2,
        specular_intensity: 0.0,
    };
    let light_entity = engine.world_mut().try_spawn(DirectionalLight {
        active: true,
        direction_to_light: Vec3::Z,
        color: light_gpu.color,
        intensity: light_gpu.intensity,
        ambient: light_gpu.ambient,
        specular_intensity: 0.0,
    })?;
    engine.tick(0.0)?;
    let first = engine
        .renderer()
        .last_mesh_result()
        .ok_or("no mesh result")?
        .clone()?;
    assert!(first.submitted);
    assert_eq!(
        (
            first.draw_calls,
            first.encoded_draw_calls,
            first.triangles,
            first.uploaded_meshes,
        ),
        (2, 1, 4, 1)
    );
    let extraction = engine.renderer().last_mesh_extraction();
    assert_eq!(extraction.eligible_lights, 1);
    assert_eq!(extraction.selected_light, Some(light_entity));
    let front_red = unorm8(shade_lambert([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], light_gpu)?);
    let front_blue = unorm8(shade_lambert([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], light_gpu)?);
    let image = engine.renderer().gpu().read_rgba()?;
    assert_eq!(image.len(), 65 * 49 * 4);
    assert_pixel_near(pixel(&image, 65, 32, 24), front_red);
    assert_pixel_near(pixel(&image, 65, 48, 24), front_blue);
    assert_eq!(pixel(&image, 65, 0, 0), [255; 4]);
    ppm(&output, 65, 49, &image)?;

    // Rotate only the near normal away from the light: the same model must fall to ambient.
    engine
        .world_mut()
        .get_mut::<Transform>(near)
        .ok_or("near transform")?
        .rotation = Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI);
    engine.tick(0.0)?;
    let back_red = unorm8(shade_lambert([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], light_gpu)?);
    assert_pixel_near(
        pixel(&engine.renderer().gpu().read_rgba()?, 65, 32, 24),
        back_red,
    );
    engine
        .world_mut()
        .get_mut::<Transform>(near)
        .ok_or("near transform")?
        .rotation = Quat::IDENTITY;

    engine.tick(0.0)?;
    assert_eq!(
        engine
            .renderer()
            .last_mesh_result()
            .ok_or("no result")?
            .clone()?
            .uploaded_meshes,
        0
    );
    assert_eq!(engine.renderer().gpu().read_rgba()?, image);
    engine.world_mut().insert(near, Visibility(false))?;
    engine.tick(0.0)?;
    assert_pixel_near(
        pixel(&engine.renderer().gpu().read_rgba()?, 65, 32, 24),
        front_blue,
    );
    engine.world_mut().insert(near, Visibility(true))?;
    engine
        .world_mut()
        .get_mut::<Transform>(camera)
        .ok_or("camera transform")?
        .translation
        .x = 0.5;
    engine.tick(0.0)?;
    assert_pixel_near(
        pixel(&engine.renderer().gpu().read_rgba()?, 65, 32, 24),
        front_blue,
    );
    engine
        .world_mut()
        .get_mut::<Transform>(camera)
        .ok_or("camera transform")?
        .translation
        .x = 0.0;
    engine.renderer_mut().gpu_mut().resize(0, 0)?;
    engine.tick(0.0)?;
    assert!(
        !engine
            .renderer()
            .last_mesh_result()
            .ok_or("no result")?
            .clone()?
            .submitted
    );
    assert!(engine.renderer().gpu().read_rgba().is_err());
    engine.renderer_mut().gpu_mut().resize(67, 51)?;
    engine.tick(0.0)?;
    assert_pixel_near(
        pixel(&engine.renderer().gpu().read_rgba()?, 67, 33, 25),
        front_red,
    );

    // Direct compatibility rendering remains ambient-only and preserves depth behavior.
    let draws = [
        MeshDraw {
            mesh: Arc::clone(&geometry),
            model: Mat4::translation(Vec3::new(-0.15, 0.0, 0.2)).data,
            color: [1.0, 0.0, 0.0, 1.0],
            shininess: 0.0,
        },
        MeshDraw {
            mesh: geometry,
            model: Mat4::translation(Vec3::new(0.15, 0.0, 0.7)).data,
            color: [0.0, 0.0, 1.0, 1.0],
            shininess: 0.0,
        },
    ];
    let gpu = engine.renderer_mut().gpu_mut();
    gpu.render(Some(Mat4::IDENTITY.data), &draws)?;
    let forward = gpu.read_rgba()?;
    gpu.render(
        Some(Mat4::IDENTITY.data),
        &[draws[1].clone(), draws[0].clone()],
    )?;
    assert_eq!(gpu.read_rgba()?, forward);
    let mut invalid = draws[0].clone();
    invalid.color[3] = 0.5;
    assert_eq!(
        gpu.render(Some(Mat4::IDENTITY.data), &[invalid]),
        Err(MeshError::InvalidColor)
    );
    assert_eq!(gpu.read_rgba()?, forward);
    engine.set_backend_extractor(|_, _| {});
    engine.tick(0.0)?;
    assert_eq!(
        engine.renderer().last_mesh_result(),
        Some(&Err(MeshError::MissingExtraction))
    );
    println!(
        "PASS: generated normals, CPU/GPU Lambert parity, consecutive instancing, normal rotation, indexed geometry, shared upload, depth order, camera, visibility, padded readback, suspend/resume, rejection"
    );
    Ok(())
}
