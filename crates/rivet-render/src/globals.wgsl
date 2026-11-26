// Shared per-frame data. Keep in sync with `Globals` in scene.rs.
struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    eye: vec4<f32>,            // w = time (s)
    sun_dir: vec4<f32>,        // direction light travels; w = shadow texel size (uv)
    sun_color: vec4<f32>,      // linear HDR
    sky_zenith: vec4<f32>,     // w = star brightness
    sky_horizon: vec4<f32>,    // w = cloud coverage 0..1
    ambient_ground: vec4<f32>,
    fog: vec4<f32>,            // rgb, a = density
    fog_params: vec4<f32>,     // x = height falloff, y = shadow depth per meter
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
};
