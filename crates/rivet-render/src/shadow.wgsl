// Depth-only pass from the sun.
struct ShadowGlobals {
    light_view_proj: mat4x4<f32>,
};
struct Model {
    model: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> sg: ShadowGlobals;
@group(1) @binding(0) var<uniform> obj: Model;

@vertex
fn vs_shadow(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return sg.light_view_proj * obj.model * vec4<f32>(pos, 1.0);
}
