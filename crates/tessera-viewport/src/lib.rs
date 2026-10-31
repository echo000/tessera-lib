//! Headless preview renderer for Tessera.
//!
//! Renders models, animations, images and materials offscreen with wgpu and
//! reads the frames back for display. Meshes are skinned on the gpu: playing
//! an animation only uploads bone matrices per frame, all pipelines and
//! vertex data are created once when a preview is set.

mod camera;
mod control;
mod error;
mod gpu;
mod image;
mod lines;
mod material;
mod model;
mod renderer;
mod target;
mod texture;

pub use control::*;
pub use error::*;
pub use material::*;
pub use renderer::*;

#[cfg(feature = "worlds")]
mod world;

#[cfg(feature = "worlds")]
pub use world::{Scene, SceneInstance};
