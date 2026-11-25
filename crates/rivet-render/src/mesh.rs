//! CPU mesh data, GPU upload, and primitive builders.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

/// Packs a material word: texture layer `a`, blend layer `b` (weighted by vertex color alpha),
/// and `flags` (`TRIPLANAR_B`, `UNTEXTURED`).
pub const fn material(a: u32, b: u32, flags: u32) -> u32 {
    (a & 255) | ((b & 255) << 8) | (flags << 16)
}

/// Layer `b` is sampled triplanar in world space (cliffs, terrain).
pub const TRIPLANAR_B: u32 = 1;
/// Ignore textures; albedo is the vertex color.
pub const UNTEXTURED: u32 = 2;
/// Self-lit, unaffected by sun and fog (boost pads, lights).
pub const EMISSIVE: u32 = 4;
/// Screen-door transparency: half the pixels are discarded (ghosts).
pub const DITHER: u32 = 8;

/// Vertex: position, normal, texture coordinates, sRGB tint (alpha = blend weight toward the
/// material's `b` layer), packed material word.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub material: u32,
}

impl Vertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Uint32
        ],
    };
}

/// Indexed triangle list, counter-clockwise front faces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Appends `other`, offsetting its indices.
    pub fn append(&mut self, other: &MeshData) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

/// A mesh uploaded to the GPU.
#[derive(Debug)]
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, data: &MeshData) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buffer,
            index_buffer,
            index_count: data.indices.len() as u32,
        }
    }
}

/// Axis-aligned box centered at the origin with flat-shaded faces.
pub fn box_mesh(half: Vec3, color: [f32; 4]) -> MeshData {
    let mut m = MeshData::default();
    for axis in 0..3 {
        for sign in [1.0f32, -1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            // Two tangents so that (u × v) = n, giving CCW winding seen from outside.
            let u = Vec3::new(n.y, n.z, n.x);
            let v = n.cross(u);
            let base = m.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = (n + u * a + v * b) * half;
                m.vertices.push(Vertex {
                    pos: p.to_array(),
                    normal: n.to_array(),
                    uv: [a * 0.5 + 0.5, b * 0.5 + 0.5],
                    color,
                    material: material(0, 0, UNTEXTURED),
                });
            }
            m.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn box_mesh_counts() {
        let m = box_mesh(Vec3::new(1.0, 2.0, 3.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m.vertices.len(), 24);
        assert_eq!(m.indices.len(), 36);
        for v in &m.vertices {
            let n = Vec3::from_array(v.normal);
            assert!((n.length() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn box_mesh_faces_point_outward() {
        let m = box_mesh(Vec3::ONE, [1.0; 4]);
        for tri in m.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(m.vertices[tri[i] as usize].pos));
            let face_n = (b - a).cross(c - a);
            let center = (a + b + c) / 3.0;
            assert!(
                face_n.dot(center) > 0.0,
                "inward-facing triangle (CCW must face out)"
            );
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use crate::scene::{Draw, FrameContent, MaterialShading, SceneRenderer, View};
    use crate::{Preset, headless_device};
    use glam::{Mat4, Vec3};
    use rivet_assets::{Rgba8Image, mip_chain};
    use rivet_platform::AdapterPreference;

    /// Renders a box through the full pipeline (shadow, HDR, sky, bloom, tonemap, FXAA) offscreen.
    #[test]
    #[ignore = "needs a GPU; run with RIVET_GPU_TESTS=1 -- --include-ignored"]
    fn scene_renderer_smoke() {
        if std::env::var("RIVET_GPU_TESTS").as_deref() != Ok("1") {
            return;
        }
        let pref: AdapterPreference = std::env::var("RIVET_ADAPTER")
            .unwrap_or_default()
            .parse()
            .unwrap();
        let (device, queue, info) = headless_device(&pref).expect("adapter");
        eprintln!("scene_renderer_smoke on {}", info.name);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let size = (64u32, 64u32);
        let tex = Rgba8Image {
            width: 4,
            height: 4,
            pixels: [200u8, 60, 40, 255].repeat(16),
        };
        let mut r = SceneRenderer::new(
            &device,
            &queue,
            format,
            size,
            Preset::Medium,
            &[mip_chain(&tex)],
            &[MaterialShading {
                specular: 0.1,
                gloss: 16.0,
                triplanar_scale: 0.1,
            }],
        );
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("smoke target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view_tex = target.create_view(&Default::default());
        let mut b = box_mesh(Vec3::splat(0.5), [1.0; 4]);
        for v in &mut b.vertices {
            v.material = material(0, 0, 0);
        }
        let mesh = GpuMesh::new(&device, &b);
        let eye = Vec3::new(0.0, 0.0, 3.0);
        let v = View {
            view: glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Y),
            proj: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0),
            eye,
            focus: Vec3::ZERO,
            time: 0.0,
        };
        let mut enc = device.create_command_encoder(&Default::default());
        let draws = [Draw {
            mesh: &mesh,
            model: Mat4::IDENTITY,
            casts_shadow: true,
        }];
        let spark = crate::ParticleInstance {
            pos: [0.0, 0.8, 0.6],
            size: 0.3,
            color: [4.0, 2.0, 0.5, 1.0],
        };
        let dust = crate::ParticleInstance {
            pos: [0.6, -0.6, 0.6],
            size: 0.4,
            color: [0.6, 0.5, 0.4, 0.5],
        };
        let content = FrameContent {
            draws: &draws,
            additive: &[spark],
            alpha: &[dust],
        };
        r.render(&device, &queue, &mut enc, &view_tex, &v, &content, None);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke readback"),
            size: 256 * 64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
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
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
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
        let center = &data[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4];
        let corner = &data[2 * 256 + 2 * 4..2 * 256 + 2 * 4 + 4];
        assert!(
            center[0] > center[2],
            "box should be reddish, got {center:?}"
        );
        assert!(
            corner[2] > corner[0],
            "corner should be blue sky, got {corner:?}"
        );
    }
}
