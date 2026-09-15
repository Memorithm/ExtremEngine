use extrem_gpu::{
    MAX_FRAME_DRAWS, MeshData, MeshDraw, MeshError, MeshVertex, validate_extent, validate_frame,
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

#[test]
fn valid_geometry_is_immutable_and_has_exact_payload_accounting() {
    let mesh = MeshData::new(vertices(), vec![0, 1, 2]).unwrap();
    assert_eq!(mesh.vertices(), vertices());
    assert_eq!(mesh.indices(), [0, 1, 2]);
    assert_eq!(mesh.payload_bytes(), 84);
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
    for (w, h) in [(0, 1), (1, 0), (u32::MAX, 2), (4096, 4096)] {
        assert_eq!(validate_extent(w, h, 4096), Err(MeshError::InvalidExtent));
    }
    assert!(validate_extent(3840, 2160, 4096).is_ok());
    assert!(validate_extent(65, 49, 4096).is_ok());
}
