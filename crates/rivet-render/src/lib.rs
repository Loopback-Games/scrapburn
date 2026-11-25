#![forbid(unsafe_code)]
//! Rivet renderer: wgpu context, GPU timing, meshes and the debug overlay.

pub mod context;
pub mod gpu_timer;
pub mod mesh;
pub mod mesh_pipeline;

pub use context::{Frame, GpuContext, PresentChoice, RenderError, headless_device};
pub use gpu_timer::GpuTimer;
pub use mesh::{GpuMesh, MeshData, Vertex, box_mesh};
pub use mesh_pipeline::{CameraUniform, MeshPipeline};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
