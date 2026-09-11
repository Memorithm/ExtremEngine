// Example demonstrating Web/WebGPU capability probing on wasm32
// This example shows how to use the WebRuntimeCapabilities to determine
// whether WebGPU is available for browser-based execution.

// Note: This example is designed to run in a wasm32 target (e.g., via wasmtime).
// On native targets, the probe will return an error indicating unsupported target.

use extrem_web::probe_web_runtime;

fn main() {
    // Attempt to probe Web/WebGPU capabilities
    match probe_web_runtime() {
        Ok(caps) => {
            println!("Web runtime capabilities:");
            println!("  Secure context: {}", caps.secure_context);
            println!("  WebGPU available: {}", caps.webgpu_available);
            println!("  WebGL available: {}", caps.webgl_available);
            println!("  Overall capable: {}", caps.is_web_capable());
            println!("  Summary: {}", caps.summary());
        }
        Err(e) => {
            println!("Failed to probe Web runtime: {:?}", e);
        }
    }
}
