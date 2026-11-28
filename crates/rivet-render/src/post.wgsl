// Post: bloom prefilter/down/up, composite (exposure, ACES, vignette), FXAA, and the menu
// backdrop (tonemapped 1/4-resolution copy, then two Kawase blurs).
struct PostParams {
    texel: vec4<f32>,  // xy = 1/source size
    a: vec4<f32>,      // x = threshold, y = knee, z = intensity, w = exposure
    b: vec4<f32>,      // x = vignette strength, y = saturation
    // The wash (composite only, `scene::Wash`): rgb = linear color, w = 1 while on.
    wash_color: vec4<f32>,
    wash_a: vec4<f32>,  // x, y = opacity at the left and right; z, w = top band opacity, height
    wash_b: vec4<f32>,  // x, y = bottom band opacity, height; z, w = right band opacity, width
};
// Kawase passes reuse `a.x` as the tap offset in source texels.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> pp: PostParams;
@group(0) @binding(3) var src2: texture_2d<f32>;

struct FsIn {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> FsIn {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: FsIn;
    o.clip = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(xy.x, 1.0 - xy.y);
    return o;
}

fn box4(uv: vec2<f32>) -> vec3<f32> {
    let t = pp.texel.xy;
    let a = textureSample(src, samp, uv + vec2<f32>(-t.x, -t.y)).rgb;
    let b = textureSample(src, samp, uv + vec2<f32>(t.x, -t.y)).rgb;
    let c = textureSample(src, samp, uv + vec2<f32>(-t.x, t.y)).rgb;
    let d = textureSample(src, samp, uv + vec2<f32>(t.x, t.y)).rgb;
    return (a + b + c + d) * 0.25;
}

@fragment
fn fs_prefilter(in: FsIn) -> @location(0) vec4<f32> {
    let c = min(box4(in.uv), vec3<f32>(40.0));
    let br = max(c.r, max(c.g, c.b));
    let knee = pp.a.y;
    var soft = clamp(br - pp.a.x + knee, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee + 1e-5);
    let contrib = max(soft, br - pp.a.x) / max(br, 1e-5);
    return vec4<f32>(c * contrib, 1.0);
}

@fragment
fn fs_down(in: FsIn) -> @location(0) vec4<f32> {
    return vec4<f32>(box4(in.uv), 1.0);
}

@fragment
fn fs_up(in: FsIn) -> @location(0) vec4<f32> {
    let t = pp.texel.xy;
    var s = textureSample(src, samp, in.uv).rgb * 4.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(-t.x, 0.0)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(t.x, 0.0)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(0.0, -t.y)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(0.0, t.y)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(-t.x, -t.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(t.x, -t.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(-t.x, t.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(t.x, t.y)).rgb;
    return vec4<f32>(s / 16.0, 1.0);
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

// HDR scene plus bloom to display color: exposure, ACES, saturation (no vignette).
fn grade(hdr: vec3<f32>, bloom: vec3<f32>) -> vec3<f32> {
    let c = aces((hdr + bloom * pp.a.z) * pp.a.w);
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luma), c, pp.b.y);
}

@fragment
fn fs_composite(in: FsIn) -> @location(0) vec4<f32> {
    let hdr = textureSample(src, samp, in.uv).rgb;
    let bloom = textureSample(src2, samp, in.uv).rgb;
    var c = grade(hdr, bloom);
    let r = length(in.uv - 0.5);
    c = c * mix(1.0, smoothstep(0.95, 0.3, r), pp.b.x);
    if (pp.wash_color.w > 0.0) {
        let u = in.uv;
        let across = mix(pp.wash_a.x, pp.wash_a.y, u.x);
        let top = pp.wash_a.z * max(1.0 - u.y / pp.wash_a.w, 0.0);
        let bottom = pp.wash_b.x * max((u.y - 1.0) / pp.wash_b.y + 1.0, 0.0);
        let right = pp.wash_b.z * max((u.x - 1.0) / pp.wash_b.w + 1.0, 0.0);
        let keep = (1.0 - across) * (1.0 - top) * (1.0 - bottom) * (1.0 - right);
        c = mix(pp.wash_color.rgb, c, keep);
    }
    return vec4<f32>(c, 1.0);
}

// Menu backdrop, first pass: downsample the HDR scene (texel = scene texel) and grade it like
// the composite. UVs keep it independent of the render scale.
@fragment
fn fs_backdrop_down(in: FsIn) -> @location(0) vec4<f32> {
    let bloom = textureSample(src2, samp, in.uv).rgb;
    return vec4<f32>(grade(box4(in.uv), bloom), 1.0);
}

// Dual-Kawase downsample kernel, run at one resolution: the center and four diagonals.
@fragment
fn fs_kawase_down(in: FsIn) -> @location(0) vec4<f32> {
    let o = pp.texel.xy * pp.a.x;
    var s = textureSample(src, samp, in.uv).rgb * 4.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(o.x, -o.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(-o.x, o.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(o.x, o.y)).rgb;
    return vec4<f32>(s / 8.0, 1.0);
}

// Dual-Kawase upsample kernel, run at one resolution: a ring of eight taps.
@fragment
fn fs_kawase_up(in: FsIn) -> @location(0) vec4<f32> {
    let o = pp.texel.xy * pp.a.x;
    var s = textureSample(src, samp, in.uv + vec2<f32>(-2.0 * o.x, 0.0)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(2.0 * o.x, 0.0)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(0.0, -2.0 * o.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(0.0, 2.0 * o.y)).rgb;
    s += textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(o.x, -o.y)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(-o.x, o.y)).rgb * 2.0;
    s += textureSample(src, samp, in.uv + vec2<f32>(o.x, o.y)).rgb * 2.0;
    return vec4<f32>(s / 12.0, 1.0);
}

fn fxaa_luma(c: vec3<f32>) -> f32 {
    return sqrt(dot(c, vec3<f32>(0.299, 0.587, 0.114)));
}

@fragment
fn fs_fxaa(in: FsIn) -> @location(0) vec4<f32> {
    let t = pp.texel.xy;
    let uv = in.uv;
    let rgb_nw = textureSample(src, samp, uv + vec2<f32>(-t.x, -t.y)).rgb;
    let rgb_ne = textureSample(src, samp, uv + vec2<f32>(t.x, -t.y)).rgb;
    let rgb_sw = textureSample(src, samp, uv + vec2<f32>(-t.x, t.y)).rgb;
    let rgb_se = textureSample(src, samp, uv + vec2<f32>(t.x, t.y)).rgb;
    let rgb_m = textureSample(src, samp, uv).rgb;
    let l_nw = fxaa_luma(rgb_nw);
    let l_ne = fxaa_luma(rgb_ne);
    let l_sw = fxaa_luma(rgb_sw);
    let l_se = fxaa_luma(rgb_se);
    let l_m = fxaa_luma(rgb_m);
    let l_min = min(l_m, min(min(l_nw, l_ne), min(l_sw, l_se)));
    let l_max = max(l_m, max(max(l_nw, l_ne), max(l_sw, l_se)));
    var dir = vec2<f32>(-((l_nw + l_ne) - (l_sw + l_se)), (l_nw + l_sw) - (l_ne + l_se));
    let reduce = max((l_nw + l_ne + l_sw + l_se) * 0.25 * (1.0 / 8.0), 1.0 / 128.0);
    let rcp_min = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
    dir = clamp(dir * rcp_min, vec2<f32>(-8.0), vec2<f32>(8.0)) * t;
    let rgb_a = 0.5 * (textureSample(src, samp, uv + dir * (1.0 / 3.0 - 0.5)).rgb
        + textureSample(src, samp, uv + dir * (2.0 / 3.0 - 0.5)).rgb);
    let rgb_b = rgb_a * 0.5 + 0.25 * (textureSample(src, samp, uv - dir * 0.5).rgb
        + textureSample(src, samp, uv + dir * 0.5).rgb);
    let l_b = fxaa_luma(rgb_b);
    if (l_b < l_min || l_b > l_max) {
        return vec4<f32>(rgb_a, 1.0);
    }
    return vec4<f32>(rgb_b, 1.0);
}
