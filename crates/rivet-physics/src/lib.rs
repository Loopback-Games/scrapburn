#![forbid(unsafe_code)]
//! Rivet physics: triangle-mesh collision queries and the arcade kart vehicle.

pub mod trimesh;
pub mod vehicle;

pub use trimesh::{Contact, RayHit, Surface, TriMesh};
pub use vehicle::{
    BoostTier, DriftState, DriveInput, VehicleParams, VehicleState, collide_karts, step,
};
