// Opaque vertex-color material with one directional Lambert light.
struct FrameUniforms {
    view_projection: mat4x4<f32>,
    light_direction_intensity: vec4<f32>,
    light_color_ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: FrameUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) tint: vec4<f32>,
) -> VertexOutput {
    let model = mat4x4<f32>(model_0, model_1, model_2, model_3);
    let c0 = model_0.xyz;
    let c1 = model_1.xyz;
    let c2 = model_2.xyz;
    let cof0 = cross(c1, c2);
    let cof1 = cross(c2, c0);
    let cof2 = cross(c0, c1);
    let orientation = select(-1.0, 1.0, dot(c0, cof0) >= 0.0);
    let normal_matrix = mat3x3<f32>(
        cof0 * orientation,
        cof1 * orientation,
        cof2 * orientation,
    );
    var output: VertexOutput;
    output.position = frame.view_projection * model * vec4<f32>(position, 1.0);
    output.color = vec4<f32>(color, 1.0) * tint;
    output.world_normal = normal_matrix * normal;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.world_normal);
    let direction_to_light = normalize(frame.light_direction_intensity.xyz);
    let diffuse = max(dot(normal, direction_to_light), 0.0) * frame.light_direction_intensity.w;
    let illumination = vec3<f32>(frame.light_color_ambient.w)
        + frame.light_color_ambient.xyz * diffuse;
    return vec4<f32>(clamp(input.color.rgb * illumination, vec3<f32>(0.0), vec3<f32>(1.0)), input.color.a);
}
