use crate::WebRuntimeCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPreference {
    LowPower,
    HighPerformance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebGpuBootstrapOptions {
    pub power_preference: PowerPreference,
    pub require_fallback_adapter: bool,
}

impl Default for WebGpuBootstrapOptions {
    fn default() -> Self {
        Self {
            power_preference: PowerPreference::HighPerformance,
            require_fallback_adapter: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebGpuBootstrapPlan {
    pub options: WebGpuBootstrapOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGpuBootstrapError {
    InsecureContext,
    WebGpuUnavailable,
}

impl WebGpuBootstrapPlan {
    pub fn from_capabilities(
        capabilities: WebRuntimeCapabilities,
        options: WebGpuBootstrapOptions,
    ) -> Result<Self, WebGpuBootstrapError> {
        if !capabilities.secure_context {
            return Err(WebGpuBootstrapError::InsecureContext);
        }
        if !capabilities.webgpu_available {
            return Err(WebGpuBootstrapError::WebGpuUnavailable);
        }
        Ok(Self { options })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_plan_requires_secure_context() {
        let capabilities = WebRuntimeCapabilities {
            secure_context: false,
            webgpu_available: true,
        };
        assert_eq!(
            WebGpuBootstrapPlan::from_capabilities(
                capabilities,
                WebGpuBootstrapOptions::default()
            ),
            Err(WebGpuBootstrapError::InsecureContext)
        );
    }

    #[test]
    fn bootstrap_plan_requires_webgpu() {
        let capabilities = WebRuntimeCapabilities {
            secure_context: true,
            webgpu_available: false,
        };
        assert_eq!(
            WebGpuBootstrapPlan::from_capabilities(
                capabilities,
                WebGpuBootstrapOptions::default()
            ),
            Err(WebGpuBootstrapError::WebGpuUnavailable)
        );
    }

    #[test]
    fn supported_runtime_produces_deterministic_plan() {
        let capabilities = WebRuntimeCapabilities {
            secure_context: true,
            webgpu_available: true,
        };
        let options = WebGpuBootstrapOptions {
            power_preference: PowerPreference::LowPower,
            require_fallback_adapter: true,
        };
        assert_eq!(
            WebGpuBootstrapPlan::from_capabilities(capabilities, options),
            Ok(WebGpuBootstrapPlan { options })
        );
    }
}
