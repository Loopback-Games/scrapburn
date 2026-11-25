// Opaque meshes: textured materials, sun + shadow, hemispheric ambient, specular, height fog.

struct Model {
    model: mat4x4<f32>,
};

struct MaterialParams {
    // x = specular strength, y = gloss exponent, z = triplanar scale (1/m)
    m: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var materials: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var shadow_map: texture_depth_2d;
@group(0) @binding(4) var shadow_samp: sampler_comparison;
@group(0) @binding(5) var<uniform> mats: MaterialParams;
@group(0) @binding(6) var clouds: texture_2d<f32>;
@group(1) @binding(0) var<uniform> obj: Model;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) material: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) @interpolate(flat) material: u32,
    @location(5) light_pos: vec4<f32>,
};

const FLAG_TRIPLANAR_B: u32 = 1u;
const FLAG_UNTEXTURED: u32 = 2u;
const FLAG_EMISSIVE: u32 = 4u;

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let world = obj.model * vec4<f32>(v.pos, 1.0);
    var o: VsOut;
    o.clip = g.view_proj * world;
    o.world = world.xyz;
    o.normal = normalize((obj.model * vec4<f32>(v.normal, 0.0)).xyz);
    o.uv = v.uv;
    o.color = vec4<f32>(pow(v.color.rgb, vec3<f32>(2.2)), v.color.a);
    o.material = v.material;
    o.light_pos = g.light_view_proj * world;
    return o;
}

fn shadow_factor(lp: vec4<f32>, n: vec3<f32>) -> f32 {
    let p = lp.xyz / lp.w;
    let uv = vec2<f32>(p.x * 0.5 + 0.5, -p.y * 0.5 + 0.5);
    if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0 || p.z >= 1.0) {
        return 1.0;
    }
    let texel = g.sun_dir.w;
    let ndl = max(dot(n, -g.sun_dir.xyz), 0.0);
    let bias = 0.0008 + 0.0025 * (1.0 - ndl);
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(f32(x), f32(y)) * texel, p.z - bias);
        }
    }
    return sum / 9.0;
}

fn triplanar(layer: i32, world: vec3<f32>, n: vec3<f32>, scale: f32, dwdx: vec3<f32>, dwdy: vec3<f32>) -> vec3<f32> {
    var w = pow(abs(n), vec3<f32>(4.0));
    w = w / (w.x + w.y + w.z);
    let x = textureSampleGrad(materials, samp, world.zy * scale, layer, dwdx.zy * scale, dwdy.zy * scale).rgb;
    let y = textureSampleGrad(materials, samp, world.xz * scale, layer, dwdx.xz * scale, dwdy.xz * scale).rgb;
    let z = textureSampleGrad(materials, samp, world.xy * scale, layer, dwdx.xy * scale, dwdy.xy * scale).rgb;
    return x * w.x + y * w.y + z * w.z;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    // Derivatives in uniform control flow, used by the gradient samples below.
    let duvdx = dpdx(in.uv);
    let duvdy = dpdy(in.uv);
    let dwdx = dpdx(in.world);
    let dwdy = dpdy(in.world);

    let la = i32(in.material & 255u);
    let lb = i32((in.material >> 8u) & 255u);
    let flags = in.material >> 16u;
    var albedo = vec3<f32>(1.0);
    if ((flags & FLAG_UNTEXTURED) == 0u) {
        let a = textureSampleGrad(materials, samp, in.uv, la, duvdx, duvdy).rgb;
        var b = a;
        if (in.color.a > 0.001) {
            if ((flags & FLAG_TRIPLANAR_B) != 0u) {
                b = triplanar(lb, in.world, n, mats.m[lb].z, dwdx, dwdy);
            } else {
                b = textureSampleGrad(materials, samp, in.uv, lb, duvdx, duvdy).rgb;
            }
        }
        albedo = mix(a, b, in.color.a);
    }
    albedo = albedo * in.color.rgb;
    if ((flags & FLAG_EMISSIVE) != 0u) {
        // Self-lit (boost pads, lights): bright enough to feed the bloom.
        return vec4<f32>(albedo * 6.0, 1.0);
    }

    let l = -g.sun_dir.xyz;
    let ndl = max(dot(n, l), 0.0);
    let sh = shadow_factor(in.light_pos, n);
    let v = normalize(g.eye.xyz - in.world);
    let h = normalize(l + v);
    let mp = mats.m[la];
    let spec = mp.x * pow(max(dot(n, h), 0.0), mp.y) * ndl;
    let sky_ambient = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, 0.5);
    let hemi = mix(g.ambient_ground.rgb, sky_ambient, n.y * 0.5 + 0.5);
    var color = albedo * (g.sun_color.rgb * ndl * sh + hemi) + g.sun_color.rgb * spec * sh;

    let d = length(g.eye.xyz - in.world);
    let height_term = exp(-max(in.world.y, 0.0) * g.fog_params.x);
    let fog_amount = 1.0 - exp(-g.fog.a * d * height_term);
    let toward_sun = pow(max(dot(-v, l), 0.0), 8.0);
    let fog_color = mix(g.fog.rgb, g.fog.rgb + g.sun_color.rgb * 0.2, toward_sun);
    color = mix(color, fog_color, fog_amount);
    return vec4<f32>(color, 1.0);
}

// ---------------------------------------------------------------------------------------------
// Sky: gradient, sun disk and glow, two layers of drifting clouds. Drawn after opaque geometry at
// the far plane so only uncovered pixels pay for it.

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(xy, 1.0, 1.0);
    o.ndc = xy;
    return o;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - g.eye.xyz);
    let up = dir.y;
    let l = -g.sun_dir.xyz;

    var sky = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, pow(clamp(up, 0.0, 1.0), 0.45));
    sky = mix(sky, g.fog.rgb, smoothstep(0.0, -0.25, up));

    let cos_sun = dot(dir, l);
    let disk = smoothstep(0.99955, 0.99975, cos_sun) * 60.0;
    let glow = pow(max(cos_sun, 0.0), 300.0) * 3.0 + pow(max(cos_sun, 0.0), 12.0) * 0.35;
    sky = sky + g.sun_color.rgb * (disk + glow);

    if (up > 0.0) {
        let t = g.eye.w;
        let proj = dir.xz / (up + 0.12);
        let n1 = textureSampleLevel(clouds, samp, proj * 0.035 + vec2<f32>(t * 0.0025, t * 0.0011), 0.0).r;
        let n2 = textureSampleLevel(clouds, samp, proj * 0.11 - vec2<f32>(t * 0.004, 0.0), 0.0).r;
        let n = n1 * 0.7 + n2 * 0.3;
        let coverage = g.sky_horizon.w;
        let density = smoothstep(1.0 - coverage, 1.0 - coverage + 0.35, n) * smoothstep(0.0, 0.18, up);
        let lit = 0.75 + 0.35 * pow(max(cos_sun, 0.0), 4.0);
        let cloud = mix(vec3<f32>(0.62, 0.66, 0.74), vec3<f32>(1.0, 0.98, 0.95), n2) * lit * 1.35;
        sky = mix(sky, cloud, density * 0.9);
    }
    return vec4<f32>(sky, 1.0);
}
