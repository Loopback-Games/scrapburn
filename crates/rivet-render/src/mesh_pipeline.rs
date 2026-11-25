//! Forward mesh pipeline: Lambert sun, hemispheric ambient, exponential fog.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::mesh::{GpuMesh, Vertex};

/// Per-frame camera and lighting. Colors are linear.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: Mat4,
    pub eye: Vec4,
    pub sun_dir: Vec4,
    pub sun_color: Vec4,
    pub ambient_sky: Vec4,
    pub ambient_ground: Vec4,
    /// rgb = fog color, w = density.
    pub fog: Vec4,
}

impl CameraUniform {
    /// Default outdoor lighting.
    pub fn new(view_proj: Mat4, eye: Vec3) -> Self {
        Self {
            view_proj,
            eye: eye.extend(1.0),
            sun_dir: Vec3::new(-0.4, -0.8, 0.35).normalize().extend(0.0),
            sun_color: Vec4::new(1.0, 0.92, 0.8, 0.0) * 2.2,
            ambient_sky: Vec4::new(0.35, 0.45, 0.65, 0.0),
            ambient_ground: Vec4::new(0.18, 0.15, 0.12, 0.0),
            fog: Vec4::new(0.55, 0.62, 0.72, 0.0015),
        }
    }
}

const MODEL_STRIDE: u64 = 256;

/// Draws meshes with per-draw model matrices via a dynamic-offset uniform buffer.
pub struct MeshPipeline {
    pipeline: wgpu::RenderPipeline,
    camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    model_bgl: wgpu::BindGroupLayout,
    model_buf: wgpu::Buffer,
    model_bg: wgpu::BindGroup,
    model_capacity: u64,
    prepared: u32,
}

fn uniform_layout(device: &wgpu::Device, label: &str, dynamic: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn model_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model uniforms"),
        size: capacity * MODEL_STRIDE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buf,
                offset: 0,
                size: wgpu::BufferSize::new(64),
            }),
        }],
    });
    (buf, bg)
}

impl MeshPipeline {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("mesh.wgsl"));
        let camera_bgl = uniform_layout(device, "camera layout", false);
        let model_bgl = uniform_layout(device, "model layout", true);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh layout"),
            bind_group_layouts: &[Some(&camera_bgl), Some(&model_bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });
        let model_capacity = 64;
        let (model_buf, model_bg) = model_buffer(device, &model_bgl, model_capacity);
        Self {
            pipeline,
            camera_buf,
            camera_bg,
            model_bgl,
            model_buf,
            model_bg,
            model_capacity,
            prepared: 0,
        }
    }

    /// Uploads the camera and one model matrix per upcoming draw. Call before the pass.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &CameraUniform,
        models: &[Mat4],
    ) {
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(camera));
        let n = models.len() as u64;
        if n > self.model_capacity {
            self.model_capacity = n.next_power_of_two();
            (self.model_buf, self.model_bg) =
                model_buffer(device, &self.model_bgl, self.model_capacity);
        }
        let mut bytes = vec![0u8; (n * MODEL_STRIDE) as usize];
        for (i, m) in models.iter().enumerate() {
            let o = i * MODEL_STRIDE as usize;
            bytes[o..o + 64].copy_from_slice(bytemuck::bytes_of(m));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.model_buf, 0, &bytes);
        }
        self.prepared = n as u32;
    }

    /// Draws `meshes[i]` with the i-th prepared model matrix.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, meshes: &[&GpuMesh]) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bg, &[]);
        for (i, mesh) in meshes.iter().enumerate().take(self.prepared as usize) {
            pass.set_bind_group(1, &self.model_bg, &[(i as u64 * MODEL_STRIDE) as u32]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEPTH_FORMAT, GpuMesh, box_mesh, headless_device};
    use glam::{Mat4, Vec3};
    use rivet_platform::AdapterPreference;

    /// Renders one box offscreen. Needs a GPU: set RIVET_GPU_TESTS=1.
    #[test]
    #[ignore = "needs a GPU; run with RIVET_GPU_TESTS=1 -- --include-ignored"]
    fn offscreen_smoke() {
        if std::env::var("RIVET_GPU_TESTS").as_deref() != Ok("1") {
            return;
        }
        let pref: AdapterPreference = std::env::var("RIVET_ADAPTER")
            .unwrap_or_default()
            .parse()
            .unwrap();
        let (device, queue, info) = headless_device(&pref).expect("adapter");
        eprintln!("offscreen_smoke on {}", info.name);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let size = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("smoke target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("smoke depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());

        let mut pipeline = MeshPipeline::new(&device, format, DEPTH_FORMAT);
        let mesh = GpuMesh::new(&device, &box_mesh(Vec3::splat(0.5), [1.0, 0.2, 0.1, 1.0]));
        let cam = CameraUniform::new(
            glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0)
                * glam::camera::rh::view::look_at_mat4(
                    Vec3::new(0.0, 0.0, 3.0),
                    Vec3::ZERO,
                    Vec3::Y,
                ),
            Vec3::new(0.0, 0.0, 3.0),
        );
        pipeline.prepare(&device, &queue, &cam, &[Mat4::IDENTITY]);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke readback"),
            size: 256 * 64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("smoke pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pipeline.draw(&mut pass, &[&mesh]);
        }
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            size,
        );
        queue.submit([enc.finish()]);
        readback.map_async(wgpu::MapMode::Read, .., |r| r.expect("map"));
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("poll");
        let data = readback.get_mapped_range(..).expect("mapped");
        let px = &data[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4];
        assert_ne!(px, &[0, 0, 0, 255], "center pixel is still the clear color");
    }
}
