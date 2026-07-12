#![forbid(unsafe_code)]

#[path = "../src/inbox/context.rs"]
mod inbox_context;

use inbox_context::{
    CANON_USAGE_CONTEXT_VERSION, ConsumerBinding, ConsumerKind, CountBand, Criticality,
    CriticalityBand, DeclaredArtifact, DeclaredProject, Exposure, ExposureBand, GroupOccurrence,
    LineageEdge, Sensitivity, TypedRole, USAGE_CONTEXT_IDENTITY_STATUS, UnresolvedGroupInput,
    UsageContextErrorCode, UsageContextInput, UsageContextPolicy, build_usage_context,
    canonical_usage_context_json_bytes,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.usage_context.v1.schema.json");

#[test]
fn schema_declares_context_only_privacy_safe_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");

    assert_eq!(schema["title"], CANON_USAGE_CONTEXT_VERSION);
    assert_eq!(
        schema["properties"]["identity_status"]["const"],
        USAGE_CONTEXT_IDENTITY_STATUS
    );
    assert_eq!(
        schema["properties"]["privacy_model"]["const"],
        "declared_inputs_banded_no_raw_values_v1"
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("dbt/search consumers")
    );
    assert_eq!(
        schema["x-canon-contract"]["decision_boundary"],
        "context and ranking support only; no identity-decision thresholds"
    );
    assert!(!schema.to_string().to_lowercase().contains("auto_merge"));
}

#[test]
fn impact_context_distinguishes_frequency_from_blast_radius_without_identity_decision() {
    let artifact = build_usage_context(sample_input()).expect("context builds");
    let high_frequency = group(&artifact, "group-high-frequency");
    let high_blast_radius = group(&artifact, "group-high-blast-radius");

    assert_eq!(artifact.version, CANON_USAGE_CONTEXT_VERSION);
    assert_eq!(artifact.identity_status, USAGE_CONTEXT_IDENTITY_STATUS);
    assert_eq!(
        high_blast_radius.identity_status,
        USAGE_CONTEXT_IDENTITY_STATUS
    );
    assert!(high_blast_radius.context_only);
    assert_eq!(high_frequency.bands.frequency, CountBand::FiftyOneToHundred);
    assert_eq!(high_blast_radius.bands.frequency, CountBand::One);
    assert_eq!(
        high_blast_radius.bands.source_criticality,
        CriticalityBand::MissionCritical
    );
    assert_eq!(high_blast_radius.bands.exposure, ExposureBand::Restricted);
    assert_eq!(
        high_blast_radius.bands.downstream_dependency_count,
        CountBand::ElevenToFifty
    );
    assert_eq!(
        high_blast_radius.bands.downstream_artifact_count,
        CountBand::One
    );
    assert_eq!(high_blast_radius.bands.lineage_depth, CountBand::One);
    assert!(
        high_blast_radius
            .consumer_kinds
            .contains(&ConsumerKind::DbtModel)
    );
    assert!(
        high_blast_radius
            .consumer_kinds
            .contains(&ConsumerKind::SearchIndex)
    );
    assert!(
        high_blast_radius.impact_units > high_frequency.impact_units,
        "declared critical downstream usage can outrank plain frequency"
    );
    assert!(
        high_blast_radius
            .contributions
            .iter()
            .any(|component| component.component == "downstream_dependencies")
    );
}

#[test]
fn sensitive_and_missing_values_are_unknown_or_banded_not_silently_zero() {
    let mut input = sample_input();
    input.groups.push(group_input("group-sensitive"));
    input.group_occurrences.push(GroupOccurrence {
        group_id: "group-sensitive".to_string(),
        artifact_id: "artifact-sensitive".to_string(),
        typed_role: TypedRole::AssignmentAssignee,
        occurrence_count: None,
    });

    let artifact = build_usage_context(input).expect("context builds");
    let sensitive = group(&artifact, "group-sensitive");

    assert_eq!(sensitive.bands.frequency, CountBand::Unknown);
    assert_eq!(sensitive.bands.row_count, CountBand::Unknown);
    assert_eq!(
        sensitive.bands.downstream_dependency_count,
        CountBand::Unknown
    );
    assert_eq!(sensitive.bands.consumer_count, CountBand::One);
    assert_eq!(
        sensitive.privacy_safe_refs.project_refs,
        Vec::<String>::new()
    );
    assert_eq!(
        sensitive.privacy_safe_refs.redacted_project_count,
        CountBand::One
    );
    assert_eq!(
        sensitive.privacy_safe_refs.redacted_artifact_count,
        CountBand::One
    );
    assert_eq!(
        sensitive.privacy_safe_refs.redacted_consumer_count,
        CountBand::One
    );
    assert!(
        sensitive
            .uncertainty_flags
            .contains(&"missing_occurrence_count".to_string())
    );
    assert!(
        sensitive
            .uncertainty_flags
            .contains(&"missing_row_count".to_string())
    );
    assert!(
        sensitive
            .uncertainty_flags
            .contains(&"missing_downstream_dependency_count".to_string())
    );
    assert!(
        sensitive
            .uncertainty_flags
            .contains(&"sensitive_context_redacted".to_string())
    );
    assert!(
        sensitive
            .contributions
            .iter()
            .any(|component| component.unknown_or_redacted)
    );
}

#[test]
fn shuffled_declared_inputs_emit_byte_identical_artifact() {
    let mut first = sample_input();
    let mut second = sample_input();

    second.groups.reverse();
    second.projects.reverse();
    second.artifacts.reverse();
    second.lineage_edges.reverse();
    second.consumer_bindings.reverse();
    second.group_occurrences.reverse();
    first.policy.weights.consumer_count = 11;
    second.policy.weights.consumer_count = 11;

    let first = build_usage_context(first).expect("first context builds");
    let second = build_usage_context(second).expect("second context builds");

    assert_eq!(
        canonical_usage_context_json_bytes(&first).unwrap(),
        canonical_usage_context_json_bytes(&second).unwrap()
    );
    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
}

#[test]
fn duplicate_and_stale_declarations_refuse_with_contract_error() {
    let mut duplicate_project = sample_input();
    duplicate_project.projects.push(DeclaredProject {
        project_id: "project-quiet".to_string(),
        sensitivity: Sensitivity::Internal,
        criticality: Some(Criticality::Low),
        exposure: Some(Exposure::Internal),
    });
    let error = build_usage_context(duplicate_project).expect_err("duplicate project refuses");
    assert_eq!(error.code, UsageContextErrorCode::ArtifactContract);
    assert!(error.message.contains("duplicate declared project id"));

    let mut stale_occurrence = sample_input();
    stale_occurrence.group_occurrences.push(GroupOccurrence {
        group_id: "group-high-frequency".to_string(),
        artifact_id: "missing-artifact".to_string(),
        typed_role: TypedRole::LookupInput,
        occurrence_count: Some(1),
    });
    let error = build_usage_context(stale_occurrence).expect_err("stale artifact refuses");
    assert_eq!(error.code, UsageContextErrorCode::ArtifactContract);
    assert!(error.message.contains("unknown artifact"));
}

#[test]
fn missing_lineage_and_consumers_are_unknown_not_zero() {
    let mut input = sample_input();
    input.lineage_edges.clear();
    input.consumer_bindings.clear();

    let artifact = build_usage_context(input).expect("context builds");
    let context = group(&artifact, "group-high-blast-radius");

    assert_eq!(context.bands.consumer_count, CountBand::Unknown);
    assert_eq!(
        context.bands.downstream_dependency_count,
        CountBand::Unknown
    );
    assert_eq!(context.bands.downstream_artifact_count, CountBand::Unknown);
    assert_eq!(context.bands.lineage_depth, CountBand::Unknown);
    assert!(
        context
            .uncertainty_flags
            .contains(&"missing_consumer_manifest".to_string())
    );
    assert!(
        context
            .uncertainty_flags
            .contains(&"missing_lineage_manifest".to_string())
    );
}

fn sample_input() -> UsageContextInput {
    UsageContextInput {
        source_unresolved_groups_artifact_hash: digest("source-groups"),
        policy: UsageContextPolicy::baseline("usage.context.policy", "rev-a"),
        groups: vec![
            group_input("group-high-blast-radius"),
            group_input("group-high-frequency"),
        ],
        projects: vec![
            DeclaredProject {
                project_id: "project-critical".to_string(),
                sensitivity: Sensitivity::Internal,
                criticality: Some(Criticality::MissionCritical),
                exposure: Some(Exposure::Restricted),
            },
            DeclaredProject {
                project_id: "project-quiet".to_string(),
                sensitivity: Sensitivity::Internal,
                criticality: Some(Criticality::Low),
                exposure: Some(Exposure::Internal),
            },
            DeclaredProject {
                project_id: "project-sensitive".to_string(),
                sensitivity: Sensitivity::Restricted,
                criticality: None,
                exposure: None,
            },
        ],
        artifacts: vec![
            DeclaredArtifact {
                artifact_id: "artifact-critical-source".to_string(),
                project_id: "project-critical".to_string(),
                artifact_kind: "apply_artifact".to_string(),
                sensitivity: Sensitivity::Internal,
                criticality: Some(Criticality::MissionCritical),
                exposure: Some(Exposure::Restricted),
                row_count: Some(1_250),
            },
            DeclaredArtifact {
                artifact_id: "artifact-critical-model".to_string(),
                project_id: "project-critical".to_string(),
                artifact_kind: "dbt_model".to_string(),
                sensitivity: Sensitivity::Internal,
                criticality: Some(Criticality::High),
                exposure: Some(Exposure::Restricted),
                row_count: Some(80_000),
            },
            DeclaredArtifact {
                artifact_id: "artifact-quiet-source".to_string(),
                project_id: "project-quiet".to_string(),
                artifact_kind: "audit_sample".to_string(),
                sensitivity: Sensitivity::Internal,
                criticality: Some(Criticality::Low),
                exposure: Some(Exposure::Internal),
                row_count: Some(120),
            },
            DeclaredArtifact {
                artifact_id: "artifact-sensitive".to_string(),
                project_id: "project-sensitive".to_string(),
                artifact_kind: "restricted_apply".to_string(),
                sensitivity: Sensitivity::Restricted,
                criticality: None,
                exposure: None,
                row_count: None,
            },
        ],
        lineage_edges: vec![LineageEdge {
            upstream_artifact_id: "artifact-critical-source".to_string(),
            downstream_artifact_id: "artifact-critical-model".to_string(),
            relation: "feeds".to_string(),
        }],
        consumer_bindings: vec![
            ConsumerBinding {
                consumer_id: "dbt-critical-model".to_string(),
                artifact_id: "artifact-critical-source".to_string(),
                consumer_kind: ConsumerKind::DbtModel,
                sensitivity: Sensitivity::Internal,
                downstream_dependency_count: Some(9),
            },
            ConsumerBinding {
                consumer_id: "search-critical".to_string(),
                artifact_id: "artifact-critical-source".to_string(),
                consumer_kind: ConsumerKind::SearchIndex,
                sensitivity: Sensitivity::Internal,
                downstream_dependency_count: Some(4),
            },
            ConsumerBinding {
                consumer_id: "restricted-search".to_string(),
                artifact_id: "artifact-sensitive".to_string(),
                consumer_kind: ConsumerKind::SearchIndex,
                sensitivity: Sensitivity::Restricted,
                downstream_dependency_count: None,
            },
        ],
        group_occurrences: vec![
            GroupOccurrence {
                group_id: "group-high-blast-radius".to_string(),
                artifact_id: "artifact-critical-source".to_string(),
                typed_role: TypedRole::AnchorField,
                occurrence_count: Some(1),
            },
            GroupOccurrence {
                group_id: "group-high-frequency".to_string(),
                artifact_id: "artifact-quiet-source".to_string(),
                typed_role: TypedRole::ContextField,
                occurrence_count: Some(75),
            },
        ],
    }
}

fn group_input(group_id: &str) -> UnresolvedGroupInput {
    UnresolvedGroupInput {
        group_id: group_id.to_string(),
        unresolved_group_digest: digest(group_id),
    }
}

fn group<'a>(
    artifact: &'a inbox_context::UsageContextArtifact,
    group_id: &str,
) -> &'a inbox_context::GroupUsageContext {
    artifact
        .groups
        .iter()
        .find(|group| group.group_id == group_id)
        .expect("group exists")
}

fn digest(value: &str) -> String {
    format!("blake3:{}", blake3::hash(value.as_bytes()).to_hex())
}
