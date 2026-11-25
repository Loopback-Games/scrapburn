//! Screen-space colored rectangles and bitmap text, drawn last in the scene pass.

use bytemuck::{Pod, Zeroable};

use crate::font;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct OverlayVertex {
    pos: [f32; 2],
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
}

impl Overlay {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("overlay.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
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
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
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
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buf.as_entire_binding(),
            }],
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

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (x1, y1) = (x + w, y + h);
        for pos in [[x, y], [x1, y], [x1, y1], [x, y], [x1, y1], [x, y1]] {
            self.vertices.push(OverlayVertex { pos, color });
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
