//! Shared entity-workbench module tree.
//!
//! `canon entity` is a registry-authoring workbench. The exact lookup kernel
//! remains outside this module and stays exact registry lookup after
//! ASCII-trim.

pub mod budget;
pub mod contracts;
pub mod error;

pub use contracts::*;
