//! Runtime implementation for the `canon entity` workbench.
//!
//! The implementation files are still stored under `src/org/` during the
//! direct namespace migration so the shared checkout does not need destructive
//! file moves. Their Rust module path is `entity::runtime`.

use crate::entity::{
    ENTITY_ARTIFACT_V1_CONTRACTS, EntityArtifactContractDescriptor, EntityArtifactStageV1,
};
use serde::Serialize;
use std::path::PathBuf;

#[path = "../org/audit.rs"]
pub mod audit;
#[path = "../org/block.rs"]
pub mod block;
#[path = "../org/edge.rs"]
pub mod edge;
#[path = "../org/explain.rs"]
pub mod explain;
#[path = "../org/incumbent.rs"]
pub mod incumbent;
#[path = "../org/output.rs"]
pub mod output;
#[path = "../org/projection.rs"]
pub mod projection;
#[path = "../org/promote.rs"]
pub mod promote;
#[path = "../org/review.rs"]
pub mod review;
#[path = "../org/solve.rs"]
pub mod solve;
#[path = "../org/strategy.rs"]
pub mod strategy;
#[path = "../org/types.rs"]
pub mod types;

pub use types::*;

pub const CANON_ENTITY_DISPATCH_PLAN_VERSION_V1: &str = "canon_entity_dispatch_plan.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityV1DispatchMode {
    Cluster,
    TwoSourceLink,
    NSourceLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityV1ProjectDispatchRequest {
    pub mode: EntityV1DispatchMode,
    pub rows: Option<PathBuf>,
    pub reference: Option<PathBuf>,
    pub target: Option<PathBuf>,
    pub profile: String,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub work_dir: PathBuf,
    pub suite: Option<PathBuf>,
}

impl EntityV1ProjectDispatchRequest {
    pub fn cluster(
        rows: PathBuf,
        profile: String,
        strategy: PathBuf,
        registry: PathBuf,
        work_dir: PathBuf,
        suite: Option<PathBuf>,
    ) -> Self {
        Self {
            mode: EntityV1DispatchMode::Cluster,
            rows: Some(rows),
            reference: None,
            target: None,
            profile,
            strategy,
            registry,
            work_dir,
            suite,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityV1LinkDispatchRequest {
    pub reference: PathBuf,
    pub target: PathBuf,
    pub profile: String,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub work_dir: PathBuf,
    pub suite: Option<PathBuf>,
}

impl From<EntityV1LinkDispatchRequest> for EntityV1ProjectDispatchRequest {
    fn from(request: EntityV1LinkDispatchRequest) -> Self {
        Self {
            mode: EntityV1DispatchMode::TwoSourceLink,
            rows: None,
            reference: Some(request.reference),
            target: Some(request.target),
            profile: request.profile,
            strategy: request.strategy,
            registry: request.registry,
            work_dir: request.work_dir,
            suite: request.suite,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityV1DispatchInputs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityV1DispatchContext {
    pub profile: String,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub work_dir: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityV1StageArtifactPlan {
    pub stage: EntityArtifactStageV1,
    pub command: &'static str,
    pub artifact_version: &'static str,
    pub stage_dir: &'static str,
    pub artifact_path: PathBuf,
    pub payload_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityV1DispatchPlan {
    pub version: &'static str,
    pub command: &'static str,
    pub requested_stage: EntityArtifactStageV1,
    pub mode: EntityV1DispatchMode,
    pub inputs: EntityV1DispatchInputs,
    pub context: EntityV1DispatchContext,
    pub artifacts: Vec<EntityV1StageArtifactPlan>,
}

impl EntityV1DispatchPlan {
    pub fn requested_artifact(&self) -> Option<&EntityV1StageArtifactPlan> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.stage == self.requested_stage)
    }
}

pub fn entity_v1_contract_for_stage(
    stage: EntityArtifactStageV1,
) -> &'static EntityArtifactContractDescriptor {
    ENTITY_ARTIFACT_V1_CONTRACTS
        .iter()
        .find(|contract| contract.stage == stage)
        .expect("every v1 entity stage has a contract descriptor")
}

pub fn entity_v1_dispatch_plan(
    stage: EntityArtifactStageV1,
    request: &EntityV1ProjectDispatchRequest,
) -> EntityV1DispatchPlan {
    let artifacts = ENTITY_ARTIFACT_V1_CONTRACTS
        .iter()
        .map(|contract| EntityV1StageArtifactPlan {
            stage: contract.stage,
            command: contract.command,
            artifact_version: contract.artifact_version,
            stage_dir: contract.stage_dir,
            artifact_path: request.work_dir.join(contract.artifact_relpath),
            payload_path: request.work_dir.join(contract.payload_relpath),
        })
        .collect();

    EntityV1DispatchPlan {
        version: CANON_ENTITY_DISPATCH_PLAN_VERSION_V1,
        command: entity_v1_contract_for_stage(stage).command,
        requested_stage: stage,
        mode: request.mode.clone(),
        inputs: EntityV1DispatchInputs {
            rows: request.rows.clone(),
            reference: request.reference.clone(),
            target: request.target.clone(),
        },
        context: EntityV1DispatchContext {
            profile: request.profile.clone(),
            strategy: request.strategy.clone(),
            registry: request.registry.clone(),
            work_dir: request.work_dir.clone(),
            suite: request.suite.clone(),
        },
        artifacts,
    }
}
