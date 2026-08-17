#![forbid(unsafe_code)]

//! Deterministic geospatial identity workbench primitives.
//!
//! Geo remains a build-time workbench. Nothing in this module changes Canon's
//! exact registry replay path.

pub mod composition;

pub use composition::*;
