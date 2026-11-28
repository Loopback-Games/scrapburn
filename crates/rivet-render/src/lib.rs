#![forbid(unsafe_code)]
//! Rivet renderer: wgpu context, GPU timing, meshes and the debug overlay.

pub mod context;
pub mod font;
pub mod font_atlas;
pub mod frame_graph;
pub mod frustum;
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
pub use frustum::Frustum;
pub use gpu_timer::GpuTimer;
pub use mesh::{
    DITHER, EMISSIVE, GpuMesh, MeshData, TRIPLANAR_B, UNTEXTURED, Vertex, box_mesh, material,
};
pub use overlay::{
    Face, FontSet, Overlay, Tracking, blend_over, label_width, label_width_face,
    label_width_tracked,
};
pub use preset::{Preset, QualitySettings};
pub use scene::{
    Draw, FrameContent, Lighting, MaterialShading, ParticleInstance, SceneRenderer, View, Wash,
};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
