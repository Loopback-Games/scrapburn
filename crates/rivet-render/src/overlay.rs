//! Screen-space colored rectangles and bitmap text, drawn last in the scene pass.

use bytemuck::{Pod, Zeroable};

use crate::font;
use crate::font_atlas::FontAtlas;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct OverlayVertex {
    pos: [f32; 2],
    /// Atlas UV; negative = solid color (no texture).
    uv: [f32; 2],
    color: [f32; 4],
}

/// Immediate-mode 2D batch in pixel coordinates, origin top-left.
pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    screen_buf: wgpu::Buffer,
    screen_bg: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    vertex_capacity: u64,
    vertices: Vec<OverlayVertex>,
    uploaded: u32,
    atlas: Option<FontAtlas>,
}

/// Font metrics shared with layout code that has no overlay at hand (HUD width measuring).
static GLOBAL_METRICS: std::sync::OnceLock<crate::font_atlas::FontMetrics> =
    std::sync::OnceLock::new();

/// Bitmap-scale units to TrueType pixels: a bitmap glyph is 7 units tall.
const LABEL_PX_PER_SCALE: f32 = 12.0;

/// Width of `label` text at `scale` (TrueType when loaded, else the bitmap font).
pub fn label_width(s: &str, scale: f32) -> f32 {
    match GLOBAL_METRICS.get() {
        Some(m) => m.text_width(s, scale * LABEL_PX_PER_SCALE),
        None => font::text_width(s, scale),
    }
}

impl Overlay {
    /// `font_ttf`: a TrueType font for `label` text (HUD); without it labels use the bitmap font.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        font_ttf: Option<&[u8]>,
    ) -> Self {
        let atlas = font_ttf.and_then(|bytes| match FontAtlas::new(bytes) {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("HUD font unusable ({e}); using the bitmap font");
                None
            }
        });
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
        let screen_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay bind group"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let vertex_capacity = 16 * 1024;
        let vertex_buf = Self::vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            screen_buf,
            screen_bg,
            vertex_buf,
            vertex_capacity,
            vertices: Vec::new(),
            uploaded: 0,
            atlas: atlas.inspect(|a| {
                let _ = GLOBAL_METRICS.set(a.metrics());
            }),
        }
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
            uv: [-1.0, -1.0],
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
                uv: [-1.0, -1.0],
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

    /// Label text whose capitals are vertically centered on `cy`.
    pub fn label_centered(&mut self, x: f32, cy: f32, scale: f32, color: [f32; 4], s: &str) {
        let y = match &self.atlas {
            Some(atlas) => cy - atlas.cap_middle(scale * LABEL_PX_PER_SCALE),
            // The bitmap font is 7 units tall from its top.
            None => cy - 3.5 * scale,
        };
        self.label(x, y, scale, color, s);
    }

    /// HUD text in the TrueType font (falls back to the bitmap font). `scale` matches `text`.
    pub fn label(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
        let Some(atlas) = &self.atlas else {
            self.text(x, y, scale, color, s);
            return;
        };
        for q in atlas.layout(s, x, y, scale * LABEL_PX_PER_SCALE) {
            let [x0, y0, w, h] = q.rect;
            let [u0, v0, u1, v1] = q.uv;
            let (x1, y1) = (x0 + w, y0 + h);
            for (pos, uv) in [
                ([x0, y0], [u0, v0]),
                ([x1, y0], [u1, v0]),
                ([x1, y1], [u1, v1]),
                ([x0, y0], [u0, v0]),
                ([x1, y1], [u1, v1]),
                ([x0, y1], [u0, v1]),
            ] {
                self.vertices.push(OverlayVertex { pos, uv, color });
            }
        }
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
        uv: [-1.0, -1.0],
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn oversized_radius_is_clamped_to_a_pill() {
        let pill = rounded_rect_vertices(0.0, 0.0, 80.0, 20.0, 999.0, [1.0; 4]);
        let clamped = rounded_rect_vertices(0.0, 0.0, 80.0, 20.0, 10.0, [1.0; 4]);
        assert_eq!(pill.len(), clamped.len());
        assert!(rounded_rect_vertices(0.0, 0.0, 0.0, 20.0, 5.0, [1.0; 4]).is_empty());
    }
}
