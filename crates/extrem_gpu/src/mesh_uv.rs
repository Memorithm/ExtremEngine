//! Validated primary texture coordinates coupled to immutable mesh geometry.
use crate::{MeshData, TextureError};
use std::sync::Arc;

/// Immutable mesh plus one UV0 coordinate per vertex.
///
/// UV values may lie outside `[0, 1]`: the current texture contract samples with clamp addressing.
/// Only finite values are accepted so malformed coordinates cannot reach shader interpolation.
#[derive(Debug)]
pub struct TexturedGeometry {
    mesh: Arc<MeshData>,
    uv0: Vec<[f32; 2]>,
}

impl TexturedGeometry {
    /// Couples validated mesh geometry with a finite UV0 channel of identical vertex count.
    ///
    /// # Examples
    /// ```
    /// use extrem_gpu::{MeshData, MeshVertex, TexturedGeometry};
    /// let mesh = MeshData::new(
    ///     vec![
    ///         MeshVertex { position: [0.0, 0.0, 0.0], color: [1.0; 3] },
    ///         MeshVertex { position: [1.0, 0.0, 0.0], color: [1.0; 3] },
    ///         MeshVertex { position: [0.0, 1.0, 0.0], color: [1.0; 3] },
    ///     ],
    ///     vec![0, 1, 2],
    /// )?;
    /// let textured = TexturedGeometry::new(
    ///     mesh,
    ///     vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
    /// )?;
    /// assert_eq!(textured.uv0()[1], [1.0, 0.0]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(mesh: Arc<MeshData>, uv0: Vec<[f32; 2]>) -> Result<Arc<Self>, TextureError> {
        if uv0.len() != mesh.vertices().len()
            || !uv0
                .iter()
                .flatten()
                .all(|coordinate| coordinate.is_finite())
        {
            return Err(TextureError::InvalidUv);
        }
        Ok(Arc::new(Self { mesh, uv0 }))
    }

    pub fn mesh(&self) -> &Arc<MeshData> {
        &self.mesh
    }

    pub fn uv0(&self) -> &[[f32; 2]] {
        &self.uv0
    }

    /// Exact CPU/GPU candidate payload for UV0 alone, excluding allocator/driver overhead.
    pub fn uv_payload_bytes(&self) -> usize {
        self.uv0.len() * std::mem::size_of::<[f32; 2]>()
    }

    /// Geometry plus UV0 payload bytes; excludes material/texture and allocator/driver overhead.
    pub fn payload_bytes(&self) -> usize {
        self.mesh.payload_bytes() + self.uv_payload_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::TexturedGeometry;
    use crate::{MeshData, MeshVertex, TextureError};
    use std::sync::Arc;

    fn triangle() -> Arc<MeshData> {
        MeshData::new(
            vec![
                MeshVertex {
                    position: [0.0, 0.0, 0.0],
                    color: [1.0; 3],
                },
                MeshVertex {
                    position: [1.0, 0.0, 0.0],
                    color: [1.0; 3],
                },
                MeshVertex {
                    position: [0.0, 1.0, 0.0],
                    color: [1.0; 3],
                },
            ],
            vec![0, 1, 2],
        )
        .unwrap()
    }

    #[test]
    fn accepts_finite_coordinates_and_preserves_mesh_identity() {
        let mesh = triangle();
        let textured = TexturedGeometry::new(
            Arc::clone(&mesh),
            vec![[-2.0, 3.0], [0.5, 0.25], [8.0, -4.0]],
        )
        .unwrap();
        assert!(Arc::ptr_eq(textured.mesh(), &mesh));
        assert_eq!(textured.uv0()[1], [0.5, 0.25]);
        assert_eq!(textured.uv_payload_bytes(), 24);
        assert_eq!(textured.payload_bytes(), mesh.payload_bytes() + 24);
    }

    #[test]
    fn rejects_missing_or_extra_coordinates() {
        let mesh = triangle();
        for uv0 in [
            vec![[0.0, 0.0]; 2],
            vec![[0.0, 0.0]; 4],
            Vec::new(),
        ] {
            assert!(matches!(
                TexturedGeometry::new(Arc::clone(&mesh), uv0),
                Err(TextureError::InvalidUv)
            ));
        }
    }

    #[test]
    fn rejects_each_nonfinite_coordinate_class() {
        let mesh = triangle();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for axis in 0..2 {
                let mut uv0 = vec![[0.0, 0.0]; 3];
                uv0[1][axis] = value;
                assert!(matches!(
                    TexturedGeometry::new(Arc::clone(&mesh), uv0),
                    Err(TextureError::InvalidUv)
                ));
            }
        }
    }
}
