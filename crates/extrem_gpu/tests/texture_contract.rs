use extrem_gpu::{
    MAX_TEXTURE_BYTES, MAX_TEXTURE_DIMENSION, MeshMaterial, TextureColorSpace, TextureData,
    TextureError,
};
use std::sync::Arc;

fn checker() -> Arc<TextureData> {
    TextureData::new_rgba8(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, // top row
            0, 0, 255, 255, 255, 255, 255, 255, // bottom row
        ],
        TextureColorSpace::Linear,
    )
    .unwrap()
}

fn approx(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn rgba8_extent_and_payload_must_match_exactly() {
    assert_eq!(
        TextureData::new_rgba8(0, 1, Vec::new(), TextureColorSpace::Linear).unwrap_err(),
        TextureError::InvalidExtent
    );
    assert_eq!(
        TextureData::new_rgba8(
            MAX_TEXTURE_DIMENSION + 1,
            1,
            Vec::new(),
            TextureColorSpace::Linear,
        )
        .unwrap_err(),
        TextureError::InvalidExtent
    );
    assert_eq!(
        TextureData::new_rgba8(2, 2, vec![0; 15], TextureColorSpace::Linear).unwrap_err(),
        TextureError::InvalidDataLength
    );
    let texture = TextureData::new_rgba8(2, 2, vec![0; 16], TextureColorSpace::Srgb).unwrap();
    assert_eq!(texture.width(), 2);
    assert_eq!(texture.height(), 2);
    assert_eq!(texture.payload_bytes(), 16);
    assert_eq!(texture.color_space(), TextureColorSpace::Srgb);
    assert!(MAX_TEXTURE_BYTES >= texture.payload_bytes());
}

#[test]
fn nearest_clamp_sampling_has_explicit_top_left_uv_contract() {
    let texture = checker();
    assert_eq!(
        texture.sample_nearest_clamp([0.0, 0.0]).unwrap(),
        [255, 0, 0, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([1.0, 0.0]).unwrap(),
        [0, 255, 0, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([0.0, 1.0]).unwrap(),
        [0, 0, 255, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([1.0, 1.0]).unwrap(),
        [255, 255, 255, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([-20.0, 20.0]).unwrap(),
        [0, 0, 255, 255]
    );
    for uv in [[f32::NAN, 0.0], [0.0, f32::INFINITY]] {
        assert_eq!(
            texture.sample_nearest_clamp(uv),
            Err(TextureError::InvalidUv)
        );
    }
}

#[test]
fn nearest_sampling_uses_normalized_texel_cell_boundaries() {
    let texture = TextureData::new_rgba8(
        3,
        1,
        vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255],
        TextureColorSpace::Linear,
    )
    .unwrap();
    assert_eq!(
        texture.sample_nearest_clamp([0.30, 0.0]).unwrap(),
        [255, 0, 0, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([0.34, 0.0]).unwrap(),
        [0, 255, 0, 255]
    );
    assert_eq!(
        texture.sample_nearest_clamp([1.0, 0.0]).unwrap(),
        [0, 0, 255, 255]
    );
}

#[test]
fn srgb_decode_is_explicit_and_alpha_stays_linear() {
    let texture =
        TextureData::new_rgba8(1, 1, vec![128, 255, 0, 128], TextureColorSpace::Srgb).unwrap();
    let sample = texture.sample_linear_nearest_clamp([0.5, 0.5]).unwrap();
    approx(sample[0], 0.215_860_53);
    approx(sample[1], 1.0);
    approx(sample[2], 0.0);
    approx(sample[3], 128.0 / 255.0);
}

#[test]
fn material_tint_is_linear_opaque_and_texture_optional() {
    let material = MeshMaterial {
        base_color: [0.5, 0.25, 1.0, 1.0],
        albedo: Some(checker()),
    };
    assert!(material.validate().is_ok());
    assert_eq!(
        material.sample_base_color([0.0, 0.0]).unwrap(),
        [0.5, 0.0, 0.0, 1.0]
    );

    let untextured = MeshMaterial {
        base_color: [0.25, 0.5, 0.75, 1.0],
        albedo: None,
    };
    assert_eq!(
        untextured.sample_base_color([0.5, 0.5]).unwrap(),
        [0.25, 0.5, 0.75, 1.0]
    );

    let transparent_texel =
        TextureData::new_rgba8(1, 1, vec![128, 64, 255, 0], TextureColorSpace::Linear).unwrap();
    let opaque_material = MeshMaterial {
        base_color: [1.0; 4],
        albedo: Some(transparent_texel),
    };
    assert_eq!(
        opaque_material.sample_base_color([0.0, 0.0]).unwrap()[3],
        1.0
    );
}

#[test]
fn material_rejects_transparency_nonfinite_and_out_of_range_values() {
    for base_color in [
        [1.0, 1.0, 1.0, 0.5],
        [f32::NAN, 1.0, 1.0, 1.0],
        [1.1, 1.0, 1.0, 1.0],
    ] {
        let material = MeshMaterial {
            base_color,
            albedo: None,
        };
        assert_eq!(material.validate(), Err(TextureError::InvalidMaterial));
    }
}
