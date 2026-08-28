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
pub mod materialize;

pub use composition::*;
pub use evaluation::*;
pub use evidence::*;
pub use geometry::*;
pub use materialize::*;
