//! CPU mesh data, GPU upload, and primitive builders.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

/// Vertex: position, normal, linear-ish sRGB color (converted in the shader).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
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
                    color,
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
