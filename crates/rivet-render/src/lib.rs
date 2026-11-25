#![forbid(unsafe_code)]
//! Rivet renderer: wgpu context, GPU timing, meshes and the debug overlay.

pub mod context;
pub mod font;
pub mod frame_graph;
pub mod gpu_timer;
pub mod mesh;
pub mod noise;
pub mod overlay;
pub mod preset;
pub mod scene;
pub mod shadow;
pub mod texture;

pub use context::{Frame, GpuContext, PresentChoice, RenderError, headless_device};
pub use frame_graph::draw_frame_graph;
pub use gpu_timer::GpuTimer;
pub use mesh::{EMISSIVE, GpuMesh, MeshData, TRIPLANAR_B, UNTEXTURED, Vertex, box_mesh, material};
pub use overlay::Overlay;
pub use preset::{Preset, QualitySettings};
pub use scene::{
    Draw, FrameContent, Lighting, MaterialShading, ParticleInstance, SceneRenderer, View,
};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
