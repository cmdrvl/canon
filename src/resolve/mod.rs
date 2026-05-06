//! Cross-tape structural resolution workbench scaffold.
//!
//! The production lookup path remains exact registry lookup. This module is the
//! bounded workbench that will turn two-tape structural evidence into flat
//! registry entries once the phase-1/phase-2 implementation beads land.

pub mod assertions;
pub mod strategy;
pub mod tape;
pub mod types;

pub use assertions::*;
pub use strategy::*;
pub use tape::*;
pub use types::*;

pub fn run(_request: ResolveRequest) -> ResolveResult<ResolveArtifact> {
    Err(ResolveError::unimplemented("canon resolve orchestration"))
}
