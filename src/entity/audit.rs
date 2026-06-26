#![forbid(unsafe_code)]

//! Audit gate runner for entity result artifacts.
//!
//! Audit is a pure boundary: it validates artifact continuity and configured
//! gates, then emits a deterministic audit artifact. Refusals happen before any
//! caller-visible mutation can occur.

use crate::{
    Refusal,
    entity::{
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
            audit_gate_refusal, validate_artifact_chain,
        },
        contracts::{
            CANON_ENTITY_AUDIT_VERSION, ENTITY_GATE_IDS, EntityArtifactHeader,
            EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        },
        error::EntityRefusalKind,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityAuditRequest {
    pub result: EntityArtifactHeader,
    pub expected: EntityArtifactChainExpectation,
    pub certified_artifacts: Vec<EntityArtifactReference>,
    pub suite: EntityAuditSuite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAuditSuite {
    pub id: String,
    pub version: String,
    pub gates: Vec<EntityAuditGateCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAuditGateCheck {
    pub gate_id: String,
    pub label: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
    #[serde(default)]
    pub evidence: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAuditArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub suite_id: String,
    pub suite_version: String,
    pub audited_artifact: EntityArtifactReference,
    pub certified_artifacts: Vec<EntityArtifactReference>,
    pub gates: Vec<EntityAuditGateResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAuditGateResult {
    pub gate_id: String,
    pub label: String,
    pub status: EntityAuditGateStatus,
    pub expected: String,
    pub actual: String,
    #[serde(default)]
    pub evidence: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityAuditGateStatus {
    Passed,
}

pub fn run_entity_audit(request: EntityAuditRequest) -> Result<EntityAuditArtifact, Refusal> {
    if request.expected.consumer_stage != EntityChainStage::Audit {
        return Err(audit_artifact_refusal(
            "Audit expectations must target the audit stage",
            json!({
                "stage": EntityChainStage::Audit.as_str(),
                "field": "consumer_stage",
                "expected": EntityChainStage::Audit.as_str(),
                "actual": request.expected.consumer_stage.as_str(),
                "writes_performed": false
            }),
        ));
    }

    let link = EntityArtifactChainLink::from_header(&request.result);
    validate_artifact_chain(&link, &request.expected)?;
    let certified_artifacts = validate_certified_artifacts(
        request.certified_artifacts,
        &request.result.version,
        &request.result.metadata.artifact_content_hash,
    )?;
    let gates = validate_audit_suite(request.suite.gates)?;

    let audited_artifact = EntityArtifactReference {
        version: request.result.version,
        content_hash: request.result.metadata.artifact_content_hash.clone(),
    };
    let mut metadata = request.result.metadata;
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = certified_artifacts.clone();

    let mut artifact = EntityAuditArtifact {
        version: CANON_ENTITY_AUDIT_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: audit_summary(
            &request.suite.id,
            &request.suite.version,
            &certified_artifacts,
            &gates,
        ),
        suite_id: request.suite.id,
        suite_version: request.suite.version,
        audited_artifact,
        certified_artifacts,
        gates,
    };
    artifact.artifact_content_hash = hash_audit_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

fn validate_certified_artifacts(
    mut artifacts: Vec<EntityArtifactReference>,
    result_version: &str,
    result_hash: &str,
) -> Result<Vec<EntityArtifactReference>, Refusal> {
    if result_hash.trim().is_empty() {
        return Err(audit_gate_preflight_refusal(
            "Audit result artifact must carry a content hash",
            json!({
                "stage": EntityChainStage::Audit.as_str(),
                "field": "result.artifact_content_hash",
                "expected": "non_empty_hash",
                "actual": result_hash,
                "writes_performed": false
            }),
        ));
    }
    for artifact in &artifacts {
        if artifact.version.trim().is_empty() || artifact.content_hash.trim().is_empty() {
            return Err(audit_gate_preflight_refusal(
                "Audit certified artifact references must be non-empty",
                json!({
                    "stage": EntityChainStage::Audit.as_str(),
                    "field": "certified_artifacts",
                    "version": artifact.version,
                    "content_hash": artifact.content_hash,
                    "expected": "non_empty_version_and_hash",
                    "actual": "missing_version_or_hash",
                    "writes_performed": false
                }),
            ));
        }
    }
    artifacts.sort_by(artifact_ref_cmp);
    artifacts.dedup();
    if !artifacts
        .iter()
        .any(|artifact| artifact.version == result_version && artifact.content_hash == result_hash)
    {
        return Err(audit_gate_preflight_refusal(
            "Audit must certify the exact result artifact being promoted",
            json!({
                "stage": EntityChainStage::Audit.as_str(),
                "field": "certified_artifacts",
                "expected": format!("{result_version}@{result_hash}"),
                "actual": artifacts
                    .iter()
                    .map(|artifact| format!("{}@{}", artifact.version, artifact.content_hash))
                    .collect::<Vec<_>>()
                    .join(","),
                "writes_performed": false
            }),
        ));
    }
    Ok(artifacts)
}

fn validate_audit_suite(
    gates: Vec<EntityAuditGateCheck>,
) -> Result<Vec<EntityAuditGateResult>, Refusal> {
    if gates.is_empty() {
        return Err(audit_artifact_refusal(
            "Audit suite must contain at least one gate",
            json!({
                "stage": EntityChainStage::Audit.as_str(),
                "field": "gates",
                "writes_performed": false
            }),
        ));
    }

    let known_gates = ENTITY_GATE_IDS.iter().copied().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut results = Vec::with_capacity(gates.len());
    for gate in gates {
        if gate.gate_id.trim().is_empty() || gate.label.trim().is_empty() {
            return Err(audit_artifact_refusal(
                "Audit gate IDs and labels must be non-empty",
                json!({
                    "stage": EntityChainStage::Audit.as_str(),
                    "field": "gate_id",
                    "writes_performed": false
                }),
            ));
        }
        if !known_gates.contains(gate.gate_id.as_str()) {
            return Err(audit_artifact_refusal(
                "Audit suite references an unknown gate",
                json!({
                    "stage": EntityChainStage::Audit.as_str(),
                    "field": "gate_id",
                    "gate_id": gate.gate_id,
                    "writes_performed": false
                }),
            ));
        }
        if !seen.insert(gate.gate_id.clone()) {
            return Err(audit_artifact_refusal(
                "Audit suite contains duplicate gates",
                json!({
                    "stage": EntityChainStage::Audit.as_str(),
                    "field": "gate_id",
                    "gate_id": gate.gate_id,
                    "writes_performed": false
                }),
            ));
        }
        if !gate.passed {
            return Err(audit_gate_refusal(gate.gate_id, gate.expected, gate.actual));
        }
        results.push(EntityAuditGateResult {
            gate_id: gate.gate_id,
            label: gate.label,
            status: EntityAuditGateStatus::Passed,
            expected: gate.expected,
            actual: gate.actual,
            evidence: gate.evidence,
        });
    }
    results.sort_by(audit_gate_result_cmp);
    Ok(results)
}

fn audit_summary(
    suite_id: &str,
    suite_version: &str,
    certified_artifacts: &[EntityArtifactReference],
    gates: &[EntityAuditGateResult],
) -> EntityDeterministicSummary {
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            (
                "certified_artifact_count".to_string(),
                certified_artifacts.len() as u64,
            ),
            ("gate_count".to_string(), gates.len() as u64),
            ("passed_gate_count".to_string(), gates.len() as u64),
            ("failed_gate_count".to_string(), 0),
        ]),
        labels: BTreeMap::from([
            ("status".to_string(), "passed".to_string()),
            ("suite_id".to_string(), suite_id.to_string()),
            ("suite_version".to_string(), suite_version.to_string()),
        ]),
    }
}

fn hash_audit_without_self(artifact: &EntityAuditArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        audit_artifact_refusal(
            "Failed to hash audit artifact",
            json!({
                "stage": EntityChainStage::Audit.as_str(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn audit_gate_result_cmp(
    left: &EntityAuditGateResult,
    right: &EntityAuditGateResult,
) -> std::cmp::Ordering {
    left.gate_id.cmp(&right.gate_id)
}

fn audit_artifact_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("canon entity audit <RESULT.json> --suite <SUITE_DIR>".to_string()),
    )
}

fn audit_gate_preflight_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::AuditGate.to_refusal(
        message,
        detail,
        Some("Fix the audited artifact set, then rerun canon entity audit".to_string()),
    )
}
