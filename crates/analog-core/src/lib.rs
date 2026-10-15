//! `analog-core`: the film grain engine library.
//!
//! This crate holds the data model (recipes and parameters), image I/O and,
//! in later milestones, the `wgpu` render pipeline that implements the four
//! effect blocks: Grain, Halation, Print Simulation and Film Color Shift.
//!
//! See `doc/index.md` for the project overview and `doc/roadmap.md` for the
//! current milestone status.

pub mod depth;
pub mod error;
pub mod film;
pub mod gpu;
pub mod grain;
pub mod halation;
pub mod image;
pub mod params;
pub mod passes;
pub mod print;
pub mod raw;
pub mod recipe;
pub mod recipe_io;
pub mod render;
pub mod shaders;

pub use depth::{DepthMap, DepthModel};
pub use error::{Error, Result};
pub use gpu::GpuContext;
pub use halation::tint_rgb;
pub use image::{Image, PixelFormat};
pub use params::{
    ALL_FILM_STOCKS, FilmParams, FilmStock, GrainParams, GrainType, HalationParams, PaperType,
    PrintParams,
};
pub use raw::{RAW_EXTENSIONS, is_raw_path};
pub use recipe::{Recipe, SCHEMA_VERSION};
pub use recipe_io::{from_recipe_xmp, to_recipe_xmp};
pub use render::{Renderer, render};
