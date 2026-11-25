#![forbid(unsafe_code)]
//! Rivet assets: image loading and CPU-side preparation (mipmaps) for the renderer.

pub mod glb;
pub mod image_data;

pub use glb::{GlbError, GlbMesh, load_glb, parse_glb};
pub use image_data::{AssetError, Rgba8Image, load_rgba, mip_chain, resized};
