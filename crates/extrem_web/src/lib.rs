#![forbid(unsafe_code)]

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebRuntimeCapabilities {
    pub secure_context: bool,
    pub webgpu_available: bool,
    pub webgl_available: bool,
}

impl WebRuntimeCapabilities {
    #[must_use]
    pub const fn can_initialize_webgpu(self) -> bool {
        self.secure_context && self.webgpu_available
    }

    #[must_use]
    pub const fn can_initialize_webgl(self) -> bool {
        self.secure_context && self.webgl_available
    }

    #[must_use]
    pub const fn is_web_capable(self) -> bool {
        self.can_initialize_webgpu() || self.can_initialize_webgl()
    }

    #[must_use]
    pub fn summary(self) -> &'static str {
        match (self.secure_context, self.webgpu_available, self.webgl_available) {
            (true, true, _) => "WebGPU ready",
            (true, false, true) => "WebGL ready",
            (true, false, false) | (false, true, _) | (false, false, true) => {
                "No GPU acceleration available"
            }
            (false, false, false) => "Insecure context",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebProbeError {
    UnsupportedTarget,
    WindowUnavailable,
    JavaScriptReflection,
}

impl fmt::Display for WebProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTarget => {
                formatter.write_str("browser capability probing requires wasm32")
            }
            Self::WindowUnavailable => formatter.write_str("browser Window is unavailable"),
            Self::JavaScriptReflection => {
                formatter.write_str("browser capability reflection failed")
            }
        }
    }
}

impl std::error::Error for WebProbeError {}

#[cfg(target_arch = "wasm32")]
pub fn probe_web_runtime() -> Result<WebRuntimeCapabilities, WebProbeError> {
    use js_sys::Reflect;
    use wasm_bindgen::JsValue;

    let window = web_sys::window().ok_or(WebProbeError::WindowUnavailable)?;
    let window_value = JsValue::from(window.clone());
    let secure_context = Reflect::get(&window_value, &JsValue::from_str("isSecureContext"))
        .map_err(|_| WebProbeError::JavaScriptReflection)?
        .as_bool()
        .unwrap_or(false);

    let navigator_value = JsValue::from(window.navigator());
    let webgpu_available = Reflect::has(&navigator_value, &JsValue::from_str("gpu"))
        .map_err(|_| WebProbeError::JavaScriptReflection)?;

    let webgl_available = {
        let has_webgl = Reflect::has(&navigator_value, &JsValue::from_str("webgl"))
            .map_err(|_| WebProbeError::JavaScriptReflection)?;
        let has_webgl2 = Reflect::has(&navigator_value, &JsValue::from_str("webgl2"))
            .map_err(|_| WebProbeError::JavaScriptReflection)?;
        has_webgl || has_webgl2
    };

    Ok(WebRuntimeCapabilities {
        secure_context,
        webgpu_available,
        webgl_available,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn probe_web_runtime() -> Result<WebRuntimeCapabilities, WebProbeError> {
    Err(WebProbeError::UnsupportedTarget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webgpu_initialization_requires_secure_context_and_gpu() {
        assert!(
            WebRuntimeCapabilities {
                secure_context: true,
                webgpu_available: true,
                webgl_available: true,
            }
            .can_initialize_webgpu()
        );

        assert!(
            !WebRuntimeCapabilities {
                secure_context: false,
                webgpu_available: true,
                webgl_available: false,
            }
            .can_initialize_webgpu()
        );

        assert!(
            !WebRuntimeCapabilities {
                secure_context: true,
                webgpu_available: false,
                webgl_available: true,
            }
            .can_initialize_webgpu()
        );
    }

    #[test]
    fn webgl_initialization_requires_secure_context_and_webgl() {
        assert!(
            WebRuntimeCapabilities {
                secure_context: true,
                webgpu_available: false,
                webgl_available: true,
            }
            .can_initialize_webgl()
        );

        assert!(
            !WebRuntimeCapabilities {
                secure_context: false,
                webgpu_available: false,
                webgl_available: true,
            }
            .can_initialize_webgl()
        );
    }

    #[test]
    fn summary_describes_capabilities() {
        assert_eq!(
            WebRuntimeCapabilities {
                secure_context: true,
                webgpu_available: true,
                webgl_available: true,
            }
            .summary(),
            "WebGPU ready"
        );
        assert_eq!(
            WebRuntimeCapabilities {
                secure_context: true,
                webgpu_available: false,
                webgl_available: true,
            }
            .summary(),
            "WebGL ready"
        );
        assert_eq!(
            WebRuntimeCapabilities {
                secure_context: false,
                webgpu_available: false,
                webgl_available: false,
            }
            .summary(),
            "Insecure context"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_probe_fails_closed() {
        assert_eq!(probe_web_runtime(), Err(WebProbeError::UnsupportedTarget));
    }
}
