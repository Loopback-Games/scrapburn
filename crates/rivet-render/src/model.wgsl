// Static vertices use only model. Posed vertices blend two local joint
// transforms with the rest position; weights are packed in the upper 12 bits.
struct Model {
    model: mat4x4<f32>,
    tint: vec4<f32>,
    joints: array<mat4x4<f32>, 2>,
};
@group(1) @binding(0) var<uniform> obj: Model;

fn posed(v: vec4<f32>, material: u32) -> vec4<f32> {
    if (obj.tint.w == 0.0 || (material >> 20u) == 0u) { return v; }
    let a = f32((material >> 20u) & 63u) / 63.0;
    let b = f32(material >> 26u) / 63.0;
    return v + (obj.joints[0] * v - v) * a + (obj.joints[1] * v - v) * b;
}
