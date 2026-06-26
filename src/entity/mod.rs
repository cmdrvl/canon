//! Shared entity-workbench module tree.
//!
//! `canon entity` is a registry-authoring workbench. The exact lookup kernel
//! remains outside this module and stays exact registry lookup after
//! ASCII-trim.

pub mod anti_merge;
pub mod apply;
pub mod artifact_chain;
pub mod audit;
pub mod block;
pub mod block_artifact;
pub mod budget;
pub mod cache;
pub mod candidates;
pub mod contracts;
pub mod diagnostics;
pub mod edge;
pub mod edge_artifact;
pub mod error;
pub mod evidence;
pub mod graph;
pub mod index;
pub mod index_io;
pub mod ledger;
pub mod ledger_replay;
pub mod patches;
pub mod postings;
pub mod prepare;
pub mod profile;
pub mod profiles;
pub mod promote;
pub mod relation;
pub mod review;
pub mod review_import;
pub mod runtime;
pub mod schema;
pub mod score;
pub mod sidecar;
pub mod solve;
pub mod stream;
pub mod surface_id;
pub mod tfidf_evidence;
pub mod topk;

pub use contracts::*;
pub use profile::EntityProfileDocument;
