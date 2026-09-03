#![forbid(unsafe_code)]

//! Deterministic geospatial identity workbench primitives.
//!
//! Geo remains a build-time workbench. Nothing in this module changes Canon's
//! exact registry replay path.

pub mod address;
pub mod assessment_roll;
pub mod cli;
pub mod composition;
pub mod condo;
pub mod control;
pub mod discovery;
pub mod evaluation;
pub mod evidence;
pub mod executor;
pub mod explain;
pub mod footprint_roll;
pub mod geometry;
pub mod geometry_value;
pub mod identifiers;
pub mod lifecycle;
pub mod materialize;
pub mod multisource;
pub mod next_evidence;
pub mod observer;
pub mod plan;
pub mod pre_resolve;
pub mod propagate;
pub mod property;
pub mod residual_benchmark;
pub mod retry;
pub mod run;
pub mod satisfy;
pub mod stack;
pub mod tile;

pub use address::*;
pub use composition::*;
pub use condo::*;
pub use control::*;
pub use discovery::*;
pub use evaluation::*;
pub use evidence::*;
pub use executor::*;
pub use explain::*;
pub use footprint_roll::*;
pub use geometry::*;
pub use geometry_value::*;
pub use identifiers::*;
pub use lifecycle::*;
pub use materialize::*;
pub use multisource::*;
pub use next_evidence::*;
pub use observer::*;
pub use plan::*;
pub use pre_resolve::*;
pub use propagate::*;
pub use property::*;
pub use residual_benchmark::*;
pub use retry::*;
pub use run::*;
pub use satisfy::*;
pub use stack::*;
pub use tile::*;
