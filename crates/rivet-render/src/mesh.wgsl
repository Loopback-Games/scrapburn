struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sun_dir: vec4<f32>,       // direction light travels
    sun_color: vec4<f32>,
    ambient_sky: vec4<f32>,
    ambient_ground: vec4<f32>,
    fog: vec4<f32>,           // rgb color, a = density
};

struct Model {
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> object: Model;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>) -> VsOut {
    let world = object.model * vec4<f32>(pos, 1.0);
    var out: VsOut;
    out.clip = camera.view_proj * world;
    out.world = world.xyz;
    out.normal = normalize((object.model * vec4<f32>(normal, 0.0)).xyz);
    out.color = vec4<f32>(pow(color.rgb, vec3<f32>(2.2)), color.a);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sun = max(dot(n, -camera.sun_dir.xyz), 0.0) * camera.sun_color.rgb;
    let hemi = mix(camera.ambient_ground.rgb, camera.ambient_sky.rgb, n.y * 0.5 + 0.5);
    var color = in.color.rgb * (sun + hemi);
    let dist = length(in.world - camera.eye.xyz);
    let fog = 1.0 - exp(-camera.fog.a * dist);
    color = mix(color, camera.fog.rgb, fog);
    return vec4<f32>(color, in.color.a);
}
