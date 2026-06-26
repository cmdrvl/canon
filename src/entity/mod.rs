//! Shared entity-workbench module tree.
//!
//! `canon entity` is a registry-authoring workbench. The exact lookup kernel
//! remains outside this module and stays exact registry lookup after
//! ASCII-trim.

pub mod artifact_chain;
pub mod block_artifact;
pub mod budget;
pub mod cache;
pub mod contracts;
pub mod edge;
pub mod error;
pub mod postings;
pub mod prepare;
pub mod profile;
pub mod profiles;
pub mod runtime;
pub mod schema;
pub mod stream;
pub mod surface_id;
pub mod topk;

pub use contracts::*;
pub use profile::EntityProfileDocument;
