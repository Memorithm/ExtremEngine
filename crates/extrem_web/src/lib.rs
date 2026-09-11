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

#[derive(Debug)]
pub enum WebSurfaceError {
    UnsupportedTarget,
    WindowUnavailable,
    JavaScriptReflection,
    CanvasNotFound,
    CanvasNotHtmlElement,
    ContextLost,
    SurfaceConfigurationFailed,
}

impl fmt::Display for WebSurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTarget => {
                formatter.write_str("browser surface creation requires wasm32")
            }
            Self::WindowUnavailable => formatter.write_str("browser Window is unavailable"),
            Self::JavaScriptReflection => {
                formatter.write_str("browser reflection failed")
            }
            Self::CanvasNotFound => {
                formatter.write_str("target canvas element not found")
            }
            Self::CanvasNotHtmlElement => {
                formatter.write_str("target element is not an HTML canvas")
            }
            Self::ContextLost => {
                formatter.write_str("GPU context lost")
            }
            Self::SurfaceConfigurationFailed => {
                formatter.write_str("surface configuration failed")
            }
        }
    }
}

impl std::error::Error for WebSurfaceError {}

/// Browser-surface descriptor for WASM‑compatible WebGPU rendering.
#[derive(Debug, Clone)]
pub struct WebSurfaceConfig {
    /// Desired CSS pixel width. Zero means "use current canvas width".
    pub width: u32,
    /// Desired CSS pixel height. Zero means "use current canvas height".
    pub height: u32,
    /// Device pixel ratio multiplier. `0.0` means query the browser.
    pub dpr: f64,
    /// Prefer high‑performance GPU power preference.
    pub power_preference: wgpu::PowerPreference,
}

impl Default for WebSurfaceConfig {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            dpr: 0.0,
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }
}

/// Visibility state of the browser page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebVisibility {
    Visible,
    Hidden,
    Prerender,
    Unknown,
}

/// Browser lifecycle snapshot used by the render loop.
#[derive(Debug, Clone)]
pub struct WebLifecycle {
    pub visibility: WebVisibility,
    pub focused: bool,
}

impl Default for WebLifecycle {
    fn default() -> Self {
        Self {
            visibility: WebVisibility::Unknown,
            focused: false,
        }
    }
}

/// Handle to the browser canvas + lifecycle state.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct WebCanvas {
    canvas: web_sys::HtmlCanvasElement,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct WebCanvas {
    _priv: (),
}

#[cfg(target_arch = "wasm32")]
impl WebCanvas {
    /// Acquire the canvas identified by `id` from the DOM.
    ///
    /// Returns `Err(WebSurfaceError::CanvasNotFound)` if no element with that
    /// `id` exists or if it is not an `HtmlCanvasElement`.
    pub fn acquire(id: &str) -> Result<Self, WebSurfaceError> {
        use wasm_bindgen::JsCast;

        let window = web_sys::window().ok_or(WebSurfaceError::WindowUnavailable)?;
        let document = window
            .document()
            .ok_or(WebSurfaceError::WindowUnavailable)?;
        let element = document
            .get_element_by_id(id)
            .ok_or(WebSurfaceError::CanvasNotFound)?;
        let canvas = element
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| WebSurfaceError::CanvasNotHtmlElement)?;

        Ok(Self { canvas })
    }

    /// Underlying `HtmlCanvasElement` reference (for surface creation).
    #[must_use]
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    /// CSS pixel width of the canvas (or 0 if unavailable).
    #[must_use]
    pub fn css_width(&self) -> u32 {
        self.canvas.client_width() as u32
    }

    /// CSS pixel height of the canvas (or 0 if unavailable).
    #[must_use]
    pub fn css_height(&self) -> u32 {
        self.canvas.client_height() as u32
    }

    /// Device pixel ratio from the browser.
    #[must_use]
    pub fn device_pixel_ratio(&self) -> f64 {
        web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0)
    }

    /// Apply resize from `WebSurfaceConfig`, returning the physical pixel dimensions.
    ///
    /// If both `cfg.width` and `cfg.height` are zero, uses the canvas CSS size.
    /// DPR is queried from the browser when `cfg.dpr <= 0.0`.
    /// Zero physical dimensions are clamped to 1×1 to satisfy wgpu surface requirements.
    pub fn configure_size(&self, cfg: &WebSurfaceConfig) -> (u32, u32) {
        let dpr = if cfg.dpr > 0.0 { cfg.dpr } else { self.device_pixel_ratio() };

        let css_w = if cfg.width > 0 { cfg.width } else { self.css_width() };
        let css_h = if cfg.height > 0 { cfg.height } else { self.css_height() };

        let phys_w = (css_w as f64 * dpr).max(1.0) as u32;
        let phys_h = (css_h as f64 * dpr).max(1.0) as u32;

        self.canvas.set_width(phys_w);
        self.canvas.set_height(phys_h);

        (phys_w, phys_h)
    }
}

/// Monotonic browser time source.
#[derive(Debug, Clone)]
pub struct BrowserClock {
    offset_seconds: f64,
}

impl BrowserClock {
    /// Build a clock whose `elapsed()` starts from 0 at call time.
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        let offset = Self::now_seconds();
        Self { offset_seconds: offset }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Self { offset_seconds: 0.0 }
    }

    #[cfg(target_arch = "wasm32")]
    fn now_seconds() -> f64 {
        web_sys::window()
            .map(|w| w.performance().map(|p| p.now() / 1000.0).unwrap_or(0.0))
            .unwrap_or(0.0)
    }

    /// Seconds elapsed since `BrowserClock::new()`.
    #[must_use]
    pub fn elapsed(&self) -> f64 {
        let now = if cfg!(target_arch = "wasm32") {
            Self::now_seconds()
        } else {
            0.0
        };
        now - self.offset_seconds
    }
}

/// Coarse event needed by the render loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebEvent {
    VisibilityChanged(WebVisibility),
    FocusChanged(bool),
    Resized { css_width: u32, css_height: u32 },
}

/// Minimal browser event‑bridge stub.
///
/// In a full implementation this would wire `VisibilityChangeEvent`,
/// `resize`, `blur`/`focus`, and pointer events.  The current
/// implementation provides the lifecycle snapshot only; event listeners are
/// expected to be wired by the embedder.
#[derive(Debug)]
pub struct WebEventBridge {
    _priv: (),
}

impl WebEventBridge {
    #[must_use]
    pub fn new() -> Self {
        Self { _priv: () }
    }

    /// Poll the current browser lifecycle snapshot.
    #[cfg(target_arch = "wasm32")]
    pub fn poll_lifecycle() -> WebLifecycle {
        let visibility = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.visibility_state().ok())
            .and_then(|s| match s.as_str() {
                "visible" => Some(WebVisibility::Visible),
                "hidden" => Some(WebVisibility::Hidden),
                "prerender" | "unloaded" => Some(WebVisibility::Prerender),
                _ => None,
            })
            .unwrap_or(WebVisibility::Unknown);

        let focused = web_sys::window()
            .and_then(|w| w.document())
            .map_or(false, |d| d.has_focus().unwrap_or(false));

        WebLifecycle { visibility, focused }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn poll_lifecycle() -> WebLifecycle {
        WebLifecycle {
            visibility: WebVisibility::Unknown,
            focused: false,
        }
    }
}

impl Default for WebEventBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WebCanvas {
    /// Acquire the canvas identified by `id` from the DOM.
    ///
    /// Always fails on native targets.
    pub fn acquire(_id: &str) -> Result<Self, WebSurfaceError> {
        Err(WebSurfaceError::UnsupportedTarget)
    }

    /// Underlying `HtmlCanvasElement` reference (for surface creation).
    #[must_use]
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        unreachable!("web canvas is only available on wasm32")
    }

    /// CSS pixel width of the canvas (or 0 if unavailable).
    #[must_use]
    pub fn css_width(&self) -> u32 {
        0
    }

    /// CSS pixel height of the canvas (or 0 if unavailable).
    #[must_use]
    pub fn css_height(&self) -> u32 {
        0
    }

    /// Device pixel ratio from the browser.
    #[must_use]
    pub fn device_pixel_ratio(&self) -> f64 {
        1.0
    }

    /// Apply resize from `WebSurfaceConfig`, returning the physical pixel dimensions.
    ///
    /// Always returns 1×1 on native targets.
    pub fn configure_size(&self, _cfg: &WebSurfaceConfig) -> (u32, u32) {
        (1, 1)
    }
}

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

    #[test]
    fn web_surface_config_defaults() {
        let cfg = WebSurfaceConfig::default();
        assert_eq!(cfg.width, 0);
        assert_eq!(cfg.height, 0);
        assert_eq!(cfg.dpr, 0.0);
        assert_eq!(cfg.power_preference, wgpu::PowerPreference::HighPerformance);
    }

    #[test]
    fn web_lifecycle_default_is_unknown() {
        let lifecycle = WebLifecycle::default();
        assert_eq!(lifecycle.visibility, WebVisibility::Unknown);
        assert!(!lifecycle.focused);
    }

    #[test]
    fn browser_clock_starts_at_zero() {
        let clock = BrowserClock::new();
        let elapsed = clock.elapsed();
        assert!(elapsed >= 0.0);
    }

    #[test]
    fn event_bridge_constructs() {
        let bridge = WebEventBridge::new();
        let _ = bridge;
    }
}
