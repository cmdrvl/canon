//! Shared entity-workbench module tree.
//!
//! `canon entity` is a registry-authoring workbench. The exact lookup kernel
//! remains outside this module and stays exact registry lookup after
//! ASCII-trim.

pub mod block_artifact;
pub mod budget;
pub mod contracts;
pub mod edge;
pub mod error;
pub mod profile;
pub mod schema;

pub use contracts::*;
