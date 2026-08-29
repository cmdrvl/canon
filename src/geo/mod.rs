#![forbid(unsafe_code)]

//! Deterministic geospatial identity workbench primitives.
//!
//! Geo remains a build-time workbench. Nothing in this module changes Canon's
//! exact registry replay path.

pub mod cli;
pub mod composition;
pub mod evaluation;
pub mod evidence;
pub mod geometry;
pub mod geometry_value;
pub mod materialize;
pub mod multisource;
pub mod tile;

pub use composition::*;
pub use evaluation::*;
pub use evidence::*;
pub use geometry::*;
pub use geometry_value::*;
pub use materialize::*;
pub use multisource::*;
pub use tile::*;
