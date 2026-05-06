//! Shared org-identity module tree.
//!
//! The initial scaffold keeps the existing exact-match lookup path unchanged
//! while giving downstream beads stable files and shared contracts to build on.

pub mod audit;
pub mod block;
pub mod edge;
pub mod explain;
pub mod incumbent;
pub mod output;
pub mod projection;
pub mod promote;
pub mod review;
pub mod solve;
pub mod strategy;
pub mod types;

pub use types::*;
