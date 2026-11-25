#![forbid(unsafe_code)]
//! Rivet physics: triangle-mesh collision queries and the arcade kart vehicle.

pub mod trimesh;
pub mod vehicle;

pub use trimesh::{Contact, RayHit, TriMesh};
pub use vehicle::{DriftState, DriveInput, VehicleParams, VehicleState, step};
