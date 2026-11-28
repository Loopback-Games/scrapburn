//! Screen-space colored shapes, gradients, glows, text, and frosted backdrop panels: one atlas,
//! one draw call. The overlay is drawn in its own pass after post-processing, so it is never
//! tonemapped or blurred itself; backdrop panels show the scene's blurred copy instead.

use bytemuck::{Pod, Zeroable};

use crate::font;
pub use crate::font_atlas::{Face, Tracking};
use crate::font_atlas::{FaceSpec, FontAtlas, FontMetrics, GlyphSet};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct OverlayVertex {
    pos: [f32; 2],
    /// Atlas UV; `SOLID_UV` = solid color, `BACKDROP_UV` = the blurred scene times the color.
    uv: [f32; 2],
    color: [f32; 4],
}

/// The uv sentinel for solid color (the shader treats -1.5..0 as solid).
const SOLID_UV: [f32; 2] = [-1.0, -1.0];
/// The uv sentinel for the backdrop (the shader treats anything below -1.5 as backdrop).
const BACKDROP_UV: [f32; 2] = [-2.0, -2.0];

/// Immediate-mode 2D batch in pixel coordinates, origin top-left.
pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    screen_buf: wgpu::Buffer,
    atlas_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// Bound at binding 3 while there is no backdrop.
    no_backdrop: wgpu::TextureView,
    /// The backdrop generation `screen_bg` was built for (`None`: built with `no_backdrop`).
    backdrop_generation: Option<u64>,
    screen_bg: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    vertex_capacity: u64,
    vertices: Vec<OverlayVertex>,
    uploaded: u32,
    atlas: Option<FontAtlas>,
    /// Atlas face index for each `Face`, if its font loaded.
    slots: [Option<usize>; 3],
}

/// Font metrics shared with layout code that has no overlay at hand (HUD width measuring).
static GLOBAL_METRICS: [std::sync::OnceLock<FontMetrics>; 3] = [
    std::sync::OnceLock::new(),
    std::sync::OnceLock::new(),
    std::sync::OnceLock::new(),
];

/// Font files for each face. A missing menu face falls back to a neighbor, never panics.
#[derive(Debug, Clone, Copy, Default)]
pub struct FontSet<'a> {
    pub hud: Option<&'a [u8]>,
    pub row: Option<&'a [u8]>,
    pub display: Option<&'a [u8]>,
}

/// Raster size of the Hud and Row faces, and of the large Display face.
const TEXT_RASTER_PX: f32 = 64.0;
const DISPLAY_RASTER_PX: f32 = 176.0;

/// The atlas specs for the available fonts, in `Face` order (Hud, Row, Display).
fn face_specs<'a>(hud: &'a [u8], row: &'a [u8], display: &'a [u8]) -> [FaceSpec<'a>; 3] {
    [
        FaceSpec {
            ttf: hud,
            raster_px: TEXT_RASTER_PX,
            glyphs: GlyphSet::Ascii,
        },
        FaceSpec {
            ttf: row,
            raster_px: TEXT_RASTER_PX,
            glyphs: GlyphSet::AsciiExtras,
        },
        FaceSpec {
            ttf: display,
            raster_px: DISPLAY_RASTER_PX,
            glyphs: GlyphSet::Caps,
        },
    ]
}

fn face_slot(face: Face) -> usize {
    match face {
        Face::Hud => 0,
        Face::Row => 1,
        Face::Display => 2,
    }
}

/// Faces to try for `face`, most preferred first.
fn fallback_chain(face: Face) -> &'static [Face] {
    match face {
        Face::Hud => &[Face::Hud],
        Face::Row => &[Face::Row, Face::Hud],
        Face::Display => &[Face::Display, Face::Row, Face::Hud],
    }
}

/// Builds an atlas from the fonts present, returning it with each face's index in it.
fn build_atlas(fonts: &FontSet<'_>) -> (Option<FontAtlas>, [Option<usize>; 3]) {
    let given = [fonts.hud, fonts.row, fonts.display];
    let all = face_specs(
        given[0].unwrap_or_default(),
        given[1].unwrap_or_default(),
        given[2].unwrap_or_default(),
    );
    let try_pack = |keep: &dyn Fn(usize) -> bool| {
        let mut slots = [None; 3];
        let mut specs = Vec::new();
        for (i, spec) in all.iter().enumerate() {
            if given[i].is_some() && keep(i) {
                slots[i] = Some(specs.len());
                specs.push(*spec);
            }
        }
        if specs.is_empty() {
            return None;
        }
        match FontAtlas::new(&specs) {
            Ok(a) => Some((a, slots)),
            Err(e) => {
                log::warn!("fonts unusable ({e})");
                None
            }
        }
    };
    // A broken menu font must not cost the HUD its face: retry with the HUD font alone.
    let packed = try_pack(&|_| true).or_else(|| try_pack(&|i| i == 0));
    match packed {
        Some((a, slots)) => (Some(a), slots),
        None => (None, [None; 3]),
    }
}

/// Bitmap-scale units to TrueType pixels: a bitmap glyph is 7 units tall.
const LABEL_PX_PER_SCALE: f32 = 12.0;

/// Width of `label` text at `scale` (TrueType when loaded, else the bitmap font).
pub fn label_width(s: &str, scale: f32) -> f32 {
    label_width_face(Face::Hud, s, scale)
}

/// Width of `label_face` text at `scale` (a missing face measures as its fallback).
pub fn label_width_face(face: Face, s: &str, scale: f32) -> f32 {
    label_width_tracked(face, s, scale, Tracking::default())
}

/// Width of `label_tracked_centered` text.
pub fn label_width_tracked(face: Face, s: &str, scale: f32, tracking: Tracking) -> f32 {
    match fallback_chain(face)
        .iter()
        .find_map(|f| GLOBAL_METRICS[face_slot(*f)].get())
    {
        Some(m) => m.text_width_tracked(s, scale * LABEL_PX_PER_SCALE, tracking),
        None if tracking == Tracking::default() => font::text_width(s, scale),
        None => {
            let mut buf = [0u8; 4];
            let mut n = 0usize;
            let w: f32 = s
                .chars()
                .map(|c| {
                    n += 1;
                    let c = if tracking.caps {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };
                    font::text_width(c.encode_utf8(&mut buf), scale)
                })
                .sum();
            w + tracking.px * n.saturating_sub(1) as f32
        }
    }
}

impl Overlay {
    /// `fonts`: TrueType fonts for `label` text; without the HUD font labels use the bitmap font.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        fonts: FontSet<'_>,
    ) -> Self {
        let (atlas, slots) = build_atlas(&fonts);
        let atlas_view = match &atlas {
            Some(a) => crate::texture::texture_2d(
                device,
                queue,
                "font atlas",
                wgpu::TextureFormat::R8Unorm,
                (a.width, a.height),
                1,
                &a.pixels,
            ),
            None => crate::texture::texture_2d(
                device,
                queue,
                "font atlas (empty)",
                wgpu::TextureFormat::R8Unorm,
                (1, 1),
                1,
                &[255],
            ),
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("font sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("overlay.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay pipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<OverlayVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                })],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let no_backdrop = crate::texture::texture_2d(
            device,
            queue,
            "no backdrop",
            wgpu::TextureFormat::Rgba8UnormSrgb,
            (1, 1),
            4,
            &[255; 4],
        );
        let screen_bg = Self::bind_group(
            device,
            &bgl,
            &screen_buf,
            &atlas_view,
            &sampler,
            &no_backdrop,
        );
        let vertex_capacity = 16 * 1024;
        let vertex_buf = Self::vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            bgl,
            screen_buf,
            atlas_view,
            sampler,
            no_backdrop,
            backdrop_generation: None,
            screen_bg,
            vertex_buf,
            vertex_capacity,
            vertices: Vec::new(),
            uploaded: 0,
            atlas: atlas.inspect(|a| {
                for (cell, slot) in GLOBAL_METRICS.iter().zip(slots) {
                    if let Some(m) = slot.and_then(|i| a.metrics(i)) {
                        let _ = cell.set(m);
                    }
                }
            }),
            slots,
        }
    }

    fn bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        screen_buf: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        backdrop: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
            ],
        })
    }

    /// Points `backdrop_quad` at `view` (the scene's blurred copy; `None` draws the tint alone).
    /// Call it every frame after the scene renders: the bind group is rebuilt only when
    /// `generation` differs from the last call's.
    pub fn set_backdrop(
        &mut self,
        device: &wgpu::Device,
        view: Option<&wgpu::TextureView>,
        generation: u64,
    ) {
        if self.backdrop_generation == Some(generation) {
            return;
        }
        self.backdrop_generation = Some(generation);
        self.screen_bg = Self::bind_group(
            device,
            &self.bgl,
            &self.screen_buf,
            &self.atlas_view,
            &self.sampler,
            view.unwrap_or(&self.no_backdrop),
        );
    }

    /// Four-point polygon (top-left, top-right, bottom-right, bottom-left) filled with the
    /// blurred scene behind it, multiplied by `tint` (alpha is the panel's opacity).
    pub fn backdrop_quad(&mut self, points: [[f32; 2]; 4], tint: [f32; 4]) {
        self.vertices.extend(backdrop_quad_vertices(points, tint));
    }

    fn vertex_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay vertices"),
            size: capacity * std::mem::size_of::<OverlayVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Solid triangle in screen pixels.
    pub fn triangle(&mut self, points: [[f32; 2]; 3], color: [f32; 4]) {
        self.vertices.extend(points.map(|pos| OverlayVertex {
            pos,
            uv: SOLID_UV,
            color,
        }));
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (x1, y1) = (x + w, y + h);
        for pos in [[x, y], [x1, y], [x1, y1], [x, y], [x1, y1], [x, y1]] {
            self.vertices.push(OverlayVertex {
                pos,
                uv: SOLID_UV,
                color,
            });
        }
    }

    /// Solid rectangle with corners rounded to `radius` (clamped to half the shorter side) and
    /// a one-pixel antialiased rim. A square with `radius` at half its side is a circle.
    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: [f32; 4]) {
        self.vertices
            .extend(rounded_rect_vertices(x, y, w, h, radius, color));
    }

    /// Solid circle of radius `r` centered on (`cx`, `cy`), with an antialiased rim.
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: [f32; 4]) {
        self.rounded_rect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, color);
    }

    /// A small glowing dot: a `core` disc of radius `r` over a fainter `glow` disc of radius
    /// `glow_r`, as two `circle`s would draw them, in one octagonal fan. For dots a few
    /// pixels across, such as embers.
    pub fn spark(&mut self, cx: f32, cy: f32, r: f32, core: [f32; 4], glow_r: f32, glow: [f32; 4]) {
        push_spark_vertices(&mut self.vertices, [cx, cy], r, core, glow_r, glow);
    }

    /// The atlas face to draw `face` with, falling back Display -> Row -> Hud.
    fn resolve(&self, face: Face) -> Option<usize> {
        fallback_chain(face)
            .iter()
            .find_map(|f| self.slots[face_slot(*f)])
    }

    /// Label text whose capitals are vertically centered on `cy`.
    pub fn label_centered(&mut self, x: f32, cy: f32, scale: f32, color: [f32; 4], s: &str) {
        self.label_face_centered(Face::Hud, x, cy, scale, color, s);
    }

    /// HUD text in the TrueType font (falls back to the bitmap font). `scale` matches `text`.
    pub fn label(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
        self.label_face(Face::Hud, x, y, scale, color, s);
    }

    /// `label_centered` in any face.
    pub fn label_face_centered(
        &mut self,
        face: Face,
        x: f32,
        cy: f32,
        scale: f32,
        color: [f32; 4],
        s: &str,
    ) {
        self.label_gradient_centered(face, x, cy, scale, color, color, s);
    }

    /// `label_gradient` with the capitals vertically centered on `cy`.
    #[allow(clippy::too_many_arguments)]
    pub fn label_gradient_centered(
        &mut self,
        face: Face,
        x: f32,
        cy: f32,
        scale: f32,
        top: [f32; 4],
        bottom: [f32; 4],
        s: &str,
    ) {
        let y = self.cap_top(face, cy, scale);
        self.label_gradient(face, x, y, scale, top, bottom, s);
    }

    /// `label` in any face. A face whose font is missing draws in the next one down.
    pub fn label_face(&mut self, face: Face, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
        self.label_gradient(face, x, y, scale, color, color, s);
    }

    /// Label text whose color fades from `top` to `bottom` over the capital height, so each
    /// glyph is shaded by where its vertices sit between the cap line and the baseline.
    #[allow(clippy::too_many_arguments)]
    pub fn label_gradient(
        &mut self,
        face: Face,
        x: f32,
        y: f32,
        scale: f32,
        top: [f32; 4],
        bottom: [f32; 4],
        s: &str,
    ) {
        self.label_run(face, x, y, scale, Tracking::default(), top, bottom, s);
    }

    /// Letter-spaced text whose capitals are vertically centered on `cy`, in one call (see
    /// `label_width_tracked` for its width).
    #[allow(clippy::too_many_arguments)]
    pub fn label_tracked_centered(
        &mut self,
        face: Face,
        x: f32,
        cy: f32,
        scale: f32,
        tracking: Tracking,
        color: [f32; 4],
        s: &str,
    ) {
        let y = self.cap_top(face, cy, scale);
        self.label_run(face, x, y, scale, tracking, color, color, s);
    }

    /// The layout `y` that centers `face` capitals on `cy`.
    fn cap_top(&self, face: Face, cy: f32, scale: f32) -> f32 {
        match (&self.atlas, self.resolve(face)) {
            (Some(atlas), Some(i)) => cy - atlas.cap_middle(i, scale * LABEL_PX_PER_SCALE),
            // The bitmap font is 7 units tall from its top.
            _ => cy - 3.5 * scale,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn label_run(
        &mut self,
        face: Face,
        x: f32,
        y: f32,
        scale: f32,
        tracking: Tracking,
        top: [f32; 4],
        bottom: [f32; 4],
        s: &str,
    ) {
        match (&self.atlas, self.resolve(face)) {
            (Some(atlas), Some(i)) => {
                let px = scale * LABEL_PX_PER_SCALE;
                push_label_vertices(
                    &mut self.vertices,
                    atlas,
                    i,
                    s,
                    [x, y],
                    px,
                    tracking,
                    top,
                    bottom,
                );
            }
            _ if tracking == Tracking::default() => self.text(x, y, scale, top, s),
            _ => {
                // The bitmap font has no tracking: set it letter by letter.
                let mut buf = [0u8; 4];
                let mut cx = x;
                for c in s.chars() {
                    let c = if tracking.caps {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };
                    let g = c.encode_utf8(&mut buf);
                    self.text(cx, y, scale, top, g);
                    cx += font::text_width(g, scale) + tracking.px;
                }
            }
        }
    }

    /// Four-point polygon (top-left, top-right, bottom-right, bottom-left) with a color at each
    /// corner. Gradient rectangles and slanted row ends are built from this.
    pub fn quad(&mut self, points: [[f32; 2]; 4], colors: [[f32; 4]; 4]) {
        self.vertices.extend(quad_vertices(points, colors));
    }

    /// Soft glow around the rectangle: `color` inside, fading to transparent over `spread`
    /// pixels outside. Draw it under the shape it lights.
    pub fn glow_rect(&mut self, x: f32, y: f32, w: f32, h: f32, spread: f32, color: [f32; 4]) {
        self.vertices
            .extend(glow_vertices(x, y, w, h, spread, color));
    }

    pub fn text(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
        for [rx, ry, rw, rh] in font::layout(s, x, y, scale) {
            self.rect(rx, ry, rw, rh, color);
        }
    }

    /// Uploads this frame's batch and clears it. Call before the pass.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        queue.write_buffer(
            &self.screen_buf,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]),
        );
        let n = self.vertices.len() as u64;
        if n > self.vertex_capacity {
            self.vertex_capacity = n.next_power_of_two();
            self.vertex_buf = Self::vertex_buffer(device, self.vertex_capacity);
        }
        if n > 0 {
            queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&self.vertices));
        }
        self.uploaded = n as u32;
        self.vertices.clear();
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.uploaded == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_bg, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.draw(0..self.uploaded, 0..1);
    }
}

/// Two triangles for a quad given as top-left, top-right, bottom-right, bottom-left.
fn quad_vertices(points: [[f32; 2]; 4], colors: [[f32; 4]; 4]) -> [OverlayVertex; 6] {
    quad_with_uv(points, colors, SOLID_UV)
}

/// `quad_vertices` sampling the backdrop instead of drawing a solid color.
fn backdrop_quad_vertices(points: [[f32; 2]; 4], tint: [f32; 4]) -> [OverlayVertex; 6] {
    quad_with_uv(points, [tint; 4], BACKDROP_UV)
}

fn quad_with_uv(points: [[f32; 2]; 4], colors: [[f32; 4]; 4], uv: [f32; 2]) -> [OverlayVertex; 6] {
    let v = |i: usize| OverlayVertex {
        pos: points[i],
        uv,
        color: colors[i],
    };
    [v(0), v(1), v(2), v(0), v(2), v(3)]
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Appends glyph triangles for `s` in atlas face `face` at `at` (left, ascent line) to `out`,
/// colored per vertex from `top` at the cap line to `bottom` at the baseline (clamped outside,
/// so descenders keep `bottom`).
#[allow(clippy::too_many_arguments)]
fn push_label_vertices(
    out: &mut Vec<OverlayVertex>,
    atlas: &FontAtlas,
    face: usize,
    s: &str,
    at: [f32; 2],
    px: f32,
    tracking: Tracking,
    top: [f32; 4],
    bottom: [f32; 4],
) {
    let [x, y] = at;
    let (cap_top, cap_h) = atlas.cap_span(face, px);
    let shade = |vy: f32| {
        if top == bottom || cap_h <= 0.0 {
            top
        } else {
            lerp4(top, bottom, ((vy - (y + cap_top)) / cap_h).clamp(0.0, 1.0))
        }
    };
    atlas.layout_with(face, s, x, y, px, tracking, |q| {
        let [x0, y0, w, h] = q.rect;
        let [u0, v0, u1, v1] = q.uv;
        let (x1, y1) = (x0 + w, y0 + h);
        let (ct, cb) = (shade(y0), shade(y1));
        for (pos, uv, color) in [
            ([x0, y0], [u0, v0], ct),
            ([x1, y0], [u1, v0], ct),
            ([x1, y1], [u1, v1], cb),
            ([x0, y0], [u0, v0], ct),
            ([x1, y1], [u1, v1], cb),
            ([x0, y1], [u0, v1], cb),
        ] {
            out.push(OverlayVertex { pos, uv, color });
        }
    });
}

/// Nine quads: the opaque center and eight slices around it whose outer vertices have alpha 0.
fn glow_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    spread: f32,
    color: [f32; 4],
) -> Vec<OverlayVertex> {
    if w <= 0.0 || h <= 0.0 || spread <= 0.0 {
        return Vec::new();
    }
    let clear = [color[0], color[1], color[2], 0.0];
    let xs = [x - spread, x, x + w, x + w + spread];
    let ys = [y - spread, y, y + h, y + h + spread];
    let mut out = Vec::with_capacity(9 * 6);
    for row in 0..3 {
        for col in 0..3 {
            let c = |i: usize, j: usize| {
                // Grid points on the inner rectangle keep the color; the rest fade out.
                if (1..=2).contains(&i) && (1..=2).contains(&j) {
                    color
                } else {
                    clear
                }
            };
            out.extend(quad_vertices(
                [
                    [xs[col], ys[row]],
                    [xs[col + 1], ys[row]],
                    [xs[col + 1], ys[row + 1]],
                    [xs[col], ys[row + 1]],
                ],
                [
                    c(col, row),
                    c(col + 1, row),
                    c(col + 1, row + 1),
                    c(col, row + 1),
                ],
            ));
        }
    }
    out
}

/// Width in pixels of the antialiased rim around rounded shapes.
const FEATHER: f32 = 1.0;

/// Points per quarter arc, enough that a corner of `radius` pixels looks smooth.
fn arc_steps(radius: f32) -> usize {
    ((radius * 0.6).ceil() as usize).clamp(2, 24)
}

/// Clockwise outline of a rounded rectangle whose corner arcs are grown by `offset` pixels.
fn rounded_outline(x: f32, y: f32, w: f32, h: f32, radius: f32, offset: f32) -> Vec<[f32; 2]> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let steps = arc_steps(radius);
    let r = (radius + offset).max(0.0);
    let corners = [
        (x + radius, y + radius, PI),
        (x + w - radius, y + radius, 1.5 * PI),
        (x + w - radius, y + h - radius, 0.0),
        (x + radius, y + h - radius, FRAC_PI_2),
    ];
    let mut points = Vec::with_capacity(4 * (steps + 1));
    for (cx, cy, start) in corners {
        for i in 0..=steps {
            let a = start + FRAC_PI_2 * i as f32 / steps as f32;
            points.push([cx + r * a.cos(), cy + r * a.sin()]);
        }
    }
    points
}

/// Triangles for `Overlay::rounded_rect`: a fan over the solid interior, then a rim that fades
/// from `color` to transparent across `FEATHER` pixels centered on the shape's edge.
fn rounded_rect_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: [f32; 4],
) -> Vec<OverlayVertex> {
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let radius = radius.clamp(0.0, 0.5 * w.min(h));
    let inner = rounded_outline(x, y, w, h, radius, -0.5 * FEATHER);
    let outer = rounded_outline(x, y, w, h, radius, 0.5 * FEATHER);
    let clear = [color[0], color[1], color[2], 0.0];
    let vertex = |pos, color| OverlayVertex {
        pos,
        uv: SOLID_UV,
        color,
    };
    let center = [x + 0.5 * w, y + 0.5 * h];
    let n = inner.len();
    let mut out = Vec::with_capacity(n * 9);
    for i in 0..n {
        let j = (i + 1) % n;
        out.extend([
            vertex(center, color),
            vertex(inner[i], color),
            vertex(inner[j], color),
            vertex(inner[i], color),
            vertex(outer[i], clear),
            vertex(outer[j], clear),
            vertex(inner[i], color),
            vertex(outer[j], clear),
            vertex(inner[j], color),
        ]);
    }
    out
}

/// Unit vectors to the eight corners of a spark's octagon.
const OCTAGON: [[f32; 2]; 8] = {
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    [
        [1.0, 0.0],
        [D, D],
        [0.0, 1.0],
        [-D, D],
        [-1.0, 0.0],
        [-D, -D],
        [0.0, -1.0],
        [D, -D],
    ]
};

/// Overlay colors are sRGB; blending happens in linear light (`overlay.wgsl` uses 2.2).
const GAMMA: f32 = 2.2;

/// `top` blended over `under`, as one color: drawing it once looks like drawing `under` and
/// then `top`.
fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let a = 1.0 - (1.0 - top[3]) * (1.0 - under[3]);
    if a <= 0.0 {
        return [top[0], top[1], top[2], 0.0];
    }
    let mix = |i: usize| {
        let lin =
            (top[3] * top[i].powf(GAMMA) + (1.0 - top[3]) * under[3] * under[i].powf(GAMMA)) / a;
        lin.powf(1.0 / GAMMA)
    };
    [mix(0), mix(1), mix(2), a]
}

/// Appends a spark (see `Overlay::spark`) to `out`: rings of an octagon from the center out,
/// with the same one-pixel rims as `rounded_rect_vertices`. 8 fan triangles and three bands
/// of 16: 168 vertices.
fn push_spark_vertices(
    out: &mut Vec<OverlayVertex>,
    center: [f32; 2],
    r: f32,
    core: [f32; 4],
    glow_r: f32,
    glow: [f32; 4],
) {
    if r <= 0.0 || glow_r <= r {
        return;
    }
    let both = over(core, glow);
    let clear = [glow[0], glow[1], glow[2], 0.0];
    let h = 0.5 * FEATHER;
    // Radius and color of each ring, from the inside out.
    let rings = [
        ((r - h).max(0.0), both),
        (r + h, glow),
        ((glow_r - h).max(r + h), glow),
        (glow_r + h, clear),
    ];
    let vertex = |radius: f32, dir: [f32; 2], color| OverlayVertex {
        pos: [center[0] + radius * dir[0], center[1] + radius * dir[1]],
        uv: SOLID_UV,
        color,
    };
    for i in 0..OCTAGON.len() {
        let (a, b) = (OCTAGON[i], OCTAGON[(i + 1) % OCTAGON.len()]);
        let (r0, c0) = rings[0];
        out.extend([vertex(0.0, a, both), vertex(r0, a, c0), vertex(r0, b, c0)]);
        for pair in rings.windows(2) {
            let ((ri, ci), (ro, co)) = (pair[0], pair[1]);
            out.extend([
                vertex(ri, a, ci),
                vertex(ro, a, co),
                vertex(ro, b, co),
                vertex(ri, a, ci),
                vertex(ro, b, co),
                vertex(ri, b, ci),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `push_label_vertices` into a new Vec, untracked.
    #[allow(clippy::too_many_arguments)]
    fn label_vertices(
        atlas: &FontAtlas,
        face: usize,
        s: &str,
        x: f32,
        y: f32,
        px: f32,
        top: [f32; 4],
        bottom: [f32; 4],
    ) -> Vec<OverlayVertex> {
        let mut out = Vec::new();
        let t = Tracking::default();
        push_label_vertices(&mut out, atlas, face, s, [x, y], px, t, top, bottom);
        out
    }

    /// Area of the opaque fan triangles (the rim triangles each have a transparent corner).
    fn fan_area(v: &[OverlayVertex]) -> f32 {
        v.chunks(3)
            .filter(|t| t.iter().all(|p| p.color[3] > 0.0))
            .map(|t| {
                let [a, b, c] = [t[0].pos, t[1].pos, t[2].pos];
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
            })
            .sum()
    }

    fn test_atlas() -> FontAtlas {
        let font = |name: &str| {
            std::fs::read(format!(
                "{}/../../assets/fonts/{name}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .expect("font in assets")
        };
        let (h, r, d) = (
            font("Rajdhani-Bold.ttf"),
            font("ChakraPetch-SemiBold.ttf"),
            font("BigShouldersStencilDisplay-Black.ttf"),
        );
        FontAtlas::new(&face_specs(&h, &r, &d)).expect("atlas")
    }

    #[derive(Debug, PartialEq)]
    enum Source {
        Backdrop,
        Solid,
        Atlas,
    }

    /// Mirrors the uv sentinel test in `overlay.wgsl`'s `fs_main`.
    fn source(v: &OverlayVertex) -> Source {
        if v.uv[0] < -1.5 {
            Source::Backdrop
        } else if v.uv[0] < 0.0 {
            Source::Solid
        } else {
            Source::Atlas
        }
    }

    #[test]
    fn uv_sentinels_select_backdrop_solid_and_atlas() {
        let pts = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let tint = [0.2, 0.1, 0.05, 0.9];
        let back = backdrop_quad_vertices(pts, tint);
        assert!(
            back.iter()
                .all(|v| source(v) == Source::Backdrop && v.color == tint)
        );
        let solid = [
            quad_vertices(pts, [tint; 4]).to_vec(),
            glow_vertices(0.0, 0.0, 10.0, 10.0, 4.0, tint),
            rounded_rect_vertices(0.0, 0.0, 10.0, 10.0, 3.0, tint),
        ];
        for v in solid.iter().flatten() {
            assert_eq!(
                source(v),
                Source::Solid,
                "solid shapes never sample the backdrop"
            );
        }
        let text = label_vertices(&test_atlas(), 0, "AB", 0.0, 0.0, 30.0, tint, tint);
        let mut spark = Vec::new();
        push_spark_vertices(&mut spark, [5.0, 5.0], 2.0, tint, 6.0, tint);
        assert!(spark.iter().all(|v| source(v) == Source::Solid));
        assert!(text.iter().all(|v| source(v) == Source::Atlas));
    }

    #[test]
    fn quad_is_two_triangles_with_per_corner_colors() {
        let pts = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let cols = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        let v = quad_vertices(pts, cols);
        assert_eq!(v.len(), 6);
        for p in &v {
            let i = pts.iter().position(|q| *q == p.pos).expect("a corner");
            assert_eq!(p.color, cols[i]);
            assert_eq!(p.uv, [-1.0, -1.0]);
        }
        let corners: Vec<_> = v.iter().map(|p| p.pos).collect();
        for q in pts {
            assert!(corners.contains(&q));
        }
    }

    #[test]
    fn gradient_label_runs_top_to_bottom_over_the_cap_height() {
        let atlas = test_atlas();
        let (top, bottom) = ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.5]);
        let (y, px) = (50.0, 120.0);
        let v = label_vertices(&atlas, 2, "HI", 5.0, y, px, top, bottom);
        assert_eq!(v.len(), 12, "two glyphs, two triangles each");
        let (cap_top, cap_h) = atlas.cap_span(2, px);
        let (mut min_t, mut max_t) = (f32::MAX, f32::MIN);
        for p in &v {
            let t = ((p.pos[1] - (y + cap_top)) / cap_h).clamp(0.0, 1.0);
            for c in 0..4 {
                let want = top[c] + (bottom[c] - top[c]) * t;
                assert!((p.color[c] - want).abs() < 1e-4, "channel {c} at t={t}");
            }
            min_t = min_t.min(t);
            max_t = max_t.max(t);
        }
        // Glyph bitmaps sit within a pixel of the cap line and baseline.
        assert!(min_t < 0.05 && max_t > 0.95, "t spans {min_t}..{max_t}");
    }

    #[test]
    fn solid_label_vertices_use_one_color() {
        let atlas = test_atlas();
        let c = [0.2, 0.4, 0.6, 1.0];
        let v = label_vertices(&atlas, 0, "OK", 0.0, 0.0, 30.0, c, c);
        assert!(v.iter().all(|p| p.color == c && p.uv[0] >= 0.0));
    }

    #[test]
    fn glow_is_nine_quads_fading_to_transparent_outside() {
        let color = [1.0, 0.5, 0.0, 0.8];
        let v = glow_vertices(100.0, 50.0, 40.0, 20.0, 12.0, color);
        assert_eq!(v.len(), 9 * 6);
        let (x0, y0, x1, y1) = (100.0, 50.0, 140.0, 70.0);
        for p in &v {
            let [px, py] = p.pos;
            let inside = (x0..=x1).contains(&px) && (y0..=y1).contains(&py);
            if inside {
                assert_eq!(p.color, color, "inner vertices keep the color");
            } else {
                assert_eq!(p.color[3], 0.0, "outer vertex at {px},{py}");
                assert_eq!(p.color[..3], color[..3]);
            }
        }
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &v {
            for a in 0..2 {
                lo[a] = lo[a].min(p.pos[a]);
                hi[a] = hi[a].max(p.pos[a]);
            }
        }
        assert_eq!((lo, hi), ([88.0, 38.0], [152.0, 82.0]));
    }

    #[test]
    fn rounded_rect_stays_within_its_bounds_plus_the_rim() {
        let v = rounded_rect_vertices(10.0, 20.0, 60.0, 24.0, 8.0, [1.0; 4]);
        assert_eq!(v.len() % 3, 0);
        let eps = 1e-3;
        for p in &v {
            let [px, py] = p.pos;
            assert!((9.5 - eps..=70.5 + eps).contains(&px), "x {px}");
            assert!((19.5 - eps..=44.5 + eps).contains(&py), "y {py}");
        }
        assert!(
            v.iter().any(|p| p.color[3] == 0.0),
            "the rim fades to transparent"
        );
    }

    #[test]
    fn circle_covers_about_pi_r_squared() {
        let r = 20.0;
        let v = rounded_rect_vertices(0.0, 0.0, 2.0 * r, 2.0 * r, r, [1.0; 4]);
        // The solid fan stops half a rim inside the edge.
        let expected = std::f32::consts::PI * (r - 0.5 * FEATHER).powi(2);
        let area = fan_area(&v);
        assert!(
            (area - expected).abs() / expected < 0.02,
            "{area} vs {expected}"
        );
    }

    #[test]
    fn tracked_labels_space_their_glyphs() {
        let atlas = test_atlas();
        let c = [1.0; 4];
        let plain = label_vertices(&atlas, 1, "ABC", 0.0, 0.0, 30.0, c, c);
        let mut spaced = Vec::new();
        let t = Tracking {
            px: 5.0,
            caps: true,
        };
        push_label_vertices(&mut spaced, &atlas, 1, "abc", [0.0, 0.0], 30.0, t, c, c);
        assert_eq!(spaced.len(), plain.len());
        for (i, (p, q)) in plain.iter().zip(&spaced).enumerate() {
            let glyph = (i / 6) as f32;
            assert!((q.pos[0] - p.pos[0] - 5.0 * glyph).abs() < 1e-3);
            assert_eq!((p.pos[1], p.uv), (q.pos[1], q.uv));
        }
    }

    #[test]
    fn a_spark_is_a_core_over_a_glow_in_one_fan() {
        let (core, glow) = ([1.0, 0.7, 0.3, 0.8], [1.0, 0.43, 0.12, 0.13]);
        let mut v = Vec::new();
        push_spark_vertices(&mut v, [100.0, 50.0], 2.0, core, 6.4, glow);
        assert_eq!(v.len(), 168);
        // Everything sits within the glow plus half a rim, and the outer ring is clear.
        let far = |p: &OverlayVertex| (p.pos[0] - 100.0).hypot(p.pos[1] - 50.0);
        assert!(v.iter().all(|p| far(p) <= 6.4 + 0.5 * FEATHER + 1e-4));
        assert!(v.iter().filter(|p| far(p) > 6.5).all(|p| p.color[3] == 0.0));
        // The center is the core blended over the glow.
        let center = v[0].color;
        assert!((center[3] - (1.0 - 0.2 * 0.87)).abs() < 1e-5);
        assert!(center[1] > glow[1] && center[1] < core[1]);
        // Between the core and the glow's rim, the glow's own color.
        let mid: Vec<_> = v.iter().filter(|p| (far(p) - 3.0).abs() < 0.6).collect();
        assert!(!mid.is_empty() && mid.iter().all(|p| p.color == glow));
        // An opaque core hides the glow entirely.
        assert_eq!(over([0.2, 0.3, 0.4, 1.0], glow), [0.2, 0.3, 0.4, 1.0]);
        // Nothing to draw without a glow larger than the core.
        let mut none = Vec::new();
        push_spark_vertices(&mut none, [0.0, 0.0], 2.0, core, 2.0, glow);
        assert!(none.is_empty());
    }

    #[test]
    fn oversized_radius_is_clamped_to_a_pill() {
        let pill = rounded_rect_vertices(0.0, 0.0, 80.0, 20.0, 999.0, [1.0; 4]);
        let clamped = rounded_rect_vertices(0.0, 0.0, 80.0, 20.0, 10.0, [1.0; 4]);
        assert_eq!(pill.len(), clamped.len());
        assert!(rounded_rect_vertices(0.0, 0.0, 0.0, 20.0, 5.0, [1.0; 4]).is_empty());
    }
}
