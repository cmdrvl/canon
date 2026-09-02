#![forbid(unsafe_code)]

//! Deterministic geospatial identity workbench primitives.
//!
//! Geo remains a build-time workbench. Nothing in this module changes Canon's
//! exact registry replay path.

pub mod address;
pub mod cli;
pub mod composition;
pub mod control;
pub mod discovery;
pub mod evaluation;
pub mod evidence;
pub mod executor;
pub mod geometry;
pub mod geometry_value;
pub mod materialize;
pub mod multisource;
pub mod plan;
pub mod residual_benchmark;
pub mod run;
pub mod satisfy;
pub mod stack;
pub mod tile;

pub use address::*;
pub use composition::*;
pub use control::*;
pub use discovery::*;
pub use evaluation::*;
pub use evidence::*;
pub use executor::*;
pub use geometry::*;
pub use geometry_value::*;
pub use materialize::*;
pub use multisource::*;
pub use plan::*;
pub use residual_benchmark::*;
pub use run::*;
pub use satisfy::*;
pub use stack::*;
pub use tile::*;
