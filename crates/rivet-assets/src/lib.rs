#![forbid(unsafe_code)]
//! Rivet assets: image loading and CPU-side preparation (mipmaps) for the renderer.

pub mod image_data;

pub use image_data::{AssetError, Rgba8Image, load_rgba, mip_chain};
