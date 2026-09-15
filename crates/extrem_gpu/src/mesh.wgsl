// Opaque vertex-color material. No physical lighting, textures or transparency.
struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) tint: vec4<f32>,
) -> VertexOutput {
    let model = mat4x4<f32>(model_0, model_1, model_2, model_3);
    var output: VertexOutput;
    output.position = camera.view_projection * model * vec4<f32>(position, 1.0);
    output.color = vec4<f32>(color, 1.0) * tint;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
