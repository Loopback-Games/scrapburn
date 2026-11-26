struct Screen {
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(pos.x / screen.size.x * 2.0 - 1.0, 1.0 - pos.y / screen.size.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.color = vec4<f32>(pow(color.rgb, vec3<f32>(2.2)), color.a);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Sample unconditionally (uniform control flow), then pick: solid rects have uv < 0.
    let coverage = textureSample(atlas, atlas_samp, max(in.uv, vec2<f32>(0.0))).r;
    let a = select(coverage, 1.0, in.uv.x < 0.0);
    return vec4<f32>(in.color.rgb, in.color.a * a);
}
