//! Native error scopes around mesh allocations; no synthetic real-OOM claim.
use crate::MeshError;

// Field drop order is innermost first, including when a Rust panic unwinds.
pub(crate) struct GpuScopes {
    validation: wgpu::ErrorScopeGuard,
    memory: wgpu::ErrorScopeGuard,
    internal: wgpu::ErrorScopeGuard,
}

impl GpuScopes {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        Self {
            validation,
            memory,
            internal,
        }
    }

    pub(crate) fn check(self) -> Result<(), MeshError> {
        // Pop ALL scopes before returning; a second error must not leave scopes
        // installed. Prefer OOM over consequent invalid-resource validation errors.
        let validation = pollster::block_on(self.validation.pop());
        let memory = pollster::block_on(self.memory.pop());
        let internal = pollster::block_on(self.internal.pop());
        classify([memory, validation, internal])
    }
}

fn classify(errors: [Option<wgpu::Error>; 3]) -> Result<(), MeshError> {
    match errors.into_iter().flatten().next() {
        Some(wgpu::Error::OutOfMemory { .. }) => Err(MeshError::OutOfMemory),
        Some(error) => Err(MeshError::Gpu(error.to_string())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_out_of_memory_takes_priority_over_consequent_validation_error() {
        let memory = wgpu::Error::OutOfMemory {
            source: std::io::Error::other("injected OOM classification test").into(),
        };
        let validation = wgpu::Error::Validation {
            source: std::io::Error::other("injected invalid resource").into(),
            description: "invalid resource after OOM".into(),
        };
        assert_eq!(
            classify([Some(memory), Some(validation), None]),
            Err(MeshError::OutOfMemory)
        );
    }

    #[test]
    fn internal_and_validation_errors_are_not_silently_successful() {
        let internal = wgpu::Error::Internal {
            source: std::io::Error::other("injected internal error").into(),
            description: "injected".into(),
        };
        assert!(matches!(
            classify([None, None, Some(internal)]),
            Err(MeshError::Gpu(_))
        ));
        assert_eq!(classify([None, None, None]), Ok(()));
    }
}
