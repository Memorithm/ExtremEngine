use extrem_gpu::{
    MAX_FRAME_DRAWS, MeshData, MeshDraw, MeshError, MeshLight, MeshVertex, shade_lambert,
    transform_normal_reference, validate_extent, validate_frame, validate_lit_frame,
};

fn vertices() -> Vec<MeshVertex> {
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
            position: [0.0, 0.5, 0.0],
            color: [1.0; 3],
        },
    ]
}

fn identity() -> [f32; 16] {
    let mut matrix = [0.0; 16];
    for index in [0, 5, 10, 15] {
        matrix[index] = 1.0;
    }
    matrix
}

fn draw() -> MeshDraw {
    MeshDraw {
        mesh: MeshData::new(vertices(), vec![0, 1, 2]).unwrap(),
        model: identity(),
        color: [1.0; 4],
    }
}

fn approx(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn generated_normals_are_normalized_and_payload_accounts_for_them() {
    let mesh = MeshData::new(vertices(), vec![0, 1, 2]).unwrap();
    assert_eq!(mesh.vertices(), vertices());
    assert_eq!(mesh.indices(), [0, 1, 2]);
    assert_eq!(mesh.payload_bytes(), 120);
    for normal in mesh.normals() {
        approx(normal[0], 0.0);
        approx(normal[1], 0.0);
        approx(normal[2], 1.0);
    }
}

#[test]
fn explicit_normals_are_normalized_and_invalid_normals_rejected() {
    let mesh =
        MeshData::new_with_normals(vertices(), vec![[0.0, 0.0, 2.0]; 3], vec![0, 1, 2]).unwrap();
    assert_eq!(mesh.normals(), [[0.0, 0.0, 1.0]; 3]);
    assert_eq!(
        MeshData::new_with_normals(vertices(), vec![[0.0, 0.0, 1.0]; 2], vec![0, 1, 2])
            .unwrap_err(),
        MeshError::InvalidNormal
    );
    for normal in [[0.0, 0.0, 0.0], [f32::NAN, 0.0, 1.0]] {
        assert_eq!(
            MeshData::new_with_normals(vertices(), vec![normal; 3], vec![0, 1, 2]).unwrap_err(),
            MeshError::InvalidNormal
        );
    }
}

#[test]
fn rejects_empty_incomplete_and_out_of_bounds_geometry() {
    assert_eq!(
        MeshData::new(Vec::new(), vec![0, 0, 0]).unwrap_err(),
        MeshError::EmptyGeometry
    );
    assert_eq!(
        MeshData::new(vertices(), Vec::new()).unwrap_err(),
        MeshError::EmptyGeometry
    );
    assert_eq!(
        MeshData::new(vertices(), vec![0, 1]).unwrap_err(),
        MeshError::InvalidIndexCount
    );
    for index in [3, u32::MAX] {
        assert_eq!(
            MeshData::new(vertices(), vec![0, 1, index]).unwrap_err(),
            MeshError::IndexOutOfBounds
        );
    }
}

#[test]
fn rejects_nonfinite_vertices_and_invalid_linear_colors() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut data = vertices();
        data[0].position[1] = value;
        assert_eq!(
            MeshData::new(data, vec![0, 1, 2]).unwrap_err(),
            MeshError::InvalidVertex
        );
    }
    for value in [-0.1, 1.1, f32::NAN] {
        let mut data = vertices();
        data[0].color[2] = value;
        assert_eq!(
            MeshData::new(data, vec![0, 1, 2]).unwrap_err(),
            MeshError::InvalidVertex
        );
    }
}

#[test]
fn alpha_blending_is_explicitly_unsupported_not_silently_opaque() {
    for alpha in [0.0, 0.5, f32::NAN] {
        let mut draw = draw();
        draw.color[3] = alpha;
        assert_eq!(draw.validate(), Err(MeshError::InvalidColor));
    }
}

#[test]
fn normal_transform_handles_nonuniform_and_mirrored_scales() {
    let mut model = identity();
    model[0] = 2.0;
    model[5] = 4.0;
    let normal = transform_normal_reference(&model, [1.0, 1.0, 0.0]).unwrap();
    let length = (0.5_f32 * 0.5 + 0.25 * 0.25).sqrt();
    approx(normal[0], 0.5 / length);
    approx(normal[1], 0.25 / length);
    let mut mirrored = identity();
    mirrored[0] = -2.0;
    assert_eq!(
        transform_normal_reference(&mirrored, [1.0, 0.0, 0.0]).unwrap(),
        [-1.0, 0.0, 0.0]
    );
    model[0] = 0.0;
    assert_eq!(
        draw_with_model(model).validate(),
        Err(MeshError::InvalidNormalTransform)
    );
}

fn draw_with_model(model: [f32; 16]) -> MeshDraw {
    MeshDraw { model, ..draw() }
}

#[test]
fn lambert_reference_has_front_back_and_colored_light_contracts() {
    let light = MeshLight {
        direction_to_light: [0.0, 0.0, 2.0],
        color: [1.0, 0.5, 0.25],
        intensity: 0.5,
        ambient: 0.25,
    };
    let front = shade_lambert([0.8, 0.4, 0.2], [0.0, 0.0, 1.0], light).unwrap();
    approx(front[0], 0.6);
    approx(front[1], 0.2);
    approx(front[2], 0.075);
    let back = shade_lambert([0.8, 0.4, 0.2], [0.0, 0.0, -1.0], light).unwrap();
    approx(back[0], 0.2);
    approx(back[1], 0.1);
    approx(back[2], 0.05);
}

#[test]
fn invalid_lights_are_rejected_before_gpu_submission() {
    let mut light = MeshLight::default();
    light.direction_to_light = [0.0; 3];
    assert_eq!(
        validate_lit_frame(&identity(), light, &[draw()]),
        Err(MeshError::InvalidLight)
    );
    let mut light = MeshLight::default();
    light.intensity = 17.0;
    assert_eq!(light.validate(), Err(MeshError::InvalidLight));
    light = MeshLight::default();
    light.ambient = f32::NAN;
    assert_eq!(light.validate(), Err(MeshError::InvalidLight));
}

#[test]
fn rejects_nonfinite_and_overflowing_camera_model_composition() {
    let mut item = draw();
    item.model[0] = f32::NAN;
    assert_eq!(
        validate_frame(&identity(), &[item]),
        Err(MeshError::InvalidMatrix)
    );
    let mut item = draw();
    item.model[0] = f32::MAX;
    let mut camera = identity();
    camera[0] = 2.0;
    assert_eq!(
        validate_frame(&camera, &[item]),
        Err(MeshError::InvalidMatrix)
    );
    camera[0] = f32::INFINITY;
    assert_eq!(validate_frame(&camera, &[]), Err(MeshError::InvalidMatrix));
}

#[test]
fn frame_count_and_extent_limits_are_checked_without_gpu_allocations() {
    let draws = vec![draw(); MAX_FRAME_DRAWS + 1];
    assert_eq!(
        validate_frame(&identity(), &draws),
        Err(MeshError::Capacity)
    );
    for (width, height) in [(0, 1), (1, 0), (u32::MAX, 2), (4096, 4096)] {
        assert_eq!(
            validate_extent(width, height, 4096),
            Err(MeshError::InvalidExtent)
        );
    }
    assert!(validate_extent(3840, 2160, 4096).is_ok());
    assert!(validate_extent(65, 49, 4096).is_ok());
}
