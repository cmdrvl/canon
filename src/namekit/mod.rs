//! Deterministic entity-name primitives for `canon entity`.
//!
//! The initial boundary is intentionally contract-first: downstream namekit
//! implementation modules must emit the stable reason-code and explain payload
//! types from `explain` instead of inventing local strings.

pub mod explain;

pub use explain::{
    NAMEKIT_EXPLAIN_VERSION, NamekitExplainTrace, NamekitReason, ReasonCode, ReasonStage,
    SourceTechnique, sort_reasons,
};
