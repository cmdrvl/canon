#![forbid(unsafe_code)]

use canon::entity::profile::EntityProfileContractEnvelope;
use canon::entity::{
    EntityClusterContractSlice, EntityContractKind, EntityGovernanceContractSlice,
    EntityLinkageContractSlice, EntityProfileDocument, EntityTypedContractErrorCode,
    EntityTypedReference, entity_profile_contract_schema_version,
};
use canon::resolve::{LinkageMapDocument, ResolveErrorCode, parse_strategy_bytes};
use serde_json::Value;
use std::collections::BTreeSet;

const PROFILE_SCHEMA_JSON: &str = include_str!("../schemas/canon.entity.profile.v1.schema.json");

#[test]
fn profile_schema_declares_typed_boundaries() {
    let schema: Value = serde_json::from_str(PROFILE_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], entity_profile_contract_schema_version());
    assert_eq!(schema["properties"]["kind"]["const"], "entity-profile");

    let kinds = schema["x-canon-contract"]["separate_contracts"]
        .as_array()
        .expect("separate contracts array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "entity-profile",
            "linkage-map",
            "evidence-policy",
            "review-policy",
            "promotion-policy",
            "frozen-executable-strategy"
        ])
    );
    assert!(
        schema["x-canon-contract"]["wrong_kind_refusal"]
            .as_str()
            .unwrap()
            .contains("refuse")
    );
}

#[test]
fn typed_profile_envelope_round_trips_without_becoming_executable() {
    let yaml = typed_profile_yaml();

    let profile = EntityProfileDocument::from_yaml_str(yaml).expect("profile still parses");
    assert_eq!(profile.profile, "tenant_identity.v1");

    let envelope =
        EntityProfileContractEnvelope::from_yaml_str(yaml).expect("typed envelope parses");
    let round_tripped = serde_json::to_value(&envelope).expect("envelope serializes");
    assert_eq!(round_tripped["kind"], "entity-profile");
    assert_eq!(round_tripped["evidence_policy"]["kind"], "evidence-policy");
    assert_eq!(
        round_tripped["frozen_executable_strategy"]["kind"],
        "frozen-executable-strategy"
    );
}

#[test]
fn entity_profile_loader_refuses_linkage_map_input() {
    let yaml = r#"
kind: linkage-map
strategy_id: legacy-link.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: loan_number
    field_tgt: loan_number
    op: exact
    weight: 1.0
match_threshold: 0.9
ambiguity_gap: 0.1
"#;

    let error = EntityProfileDocument::from_yaml_str(yaml).expect_err("wrong kind rejects");
    assert_eq!(error.detail["expected_kind"], "entity-profile");
    assert_eq!(error.detail["actual_kind"], "linkage-map");
}

#[test]
fn resolve_loader_refuses_profile_and_frozen_strategy_documents() {
    let profile_error =
        parse_strategy_bytes(typed_profile_yaml().as_bytes()).expect_err("profile rejects");
    assert_eq!(profile_error.code, ResolveErrorCode::Strategy);
    assert_eq!(
        profile_error.detail.unwrap()["actual_kind"],
        Value::String("entity-profile".to_string())
    );

    let frozen = r#"
kind: frozen-executable-strategy
id: tenant-linker.py
version: "2026.07.10"
language: python
script_id: tenant-linker.v1
entrypoint: run
content_hash: blake3:frozen-script
"#;
    let frozen_error = parse_strategy_bytes(frozen.as_bytes()).expect_err("frozen rejects");
    assert_eq!(frozen_error.code, ResolveErrorCode::Strategy);
    assert_eq!(
        frozen_error.detail.unwrap()["actual_kind"],
        Value::String("frozen-executable-strategy".to_string())
    );
}

#[test]
fn legacy_linkage_yaml_has_a_lossless_typed_destination() {
    let yaml = r#"
strategy_id: legacy-link.v1
strategy_version: "0.1.0"
entity_type: loan
description: legacy linkage fixture
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
candidate_filter:
  - field_ref: servicer
    field_tgt: servicer
    op: exact
    weight: 0.2
assertions:
  - field_ref: loan_number
    field_tgt: loan_number
    op: exact
    weight: 1.0
match_threshold: 0.9
ambiguity_gap: 0.1
"#;

    let strategy = parse_strategy_bytes(yaml.as_bytes()).expect("legacy linkage parses");
    let document = LinkageMapDocument::from_legacy_strategy(strategy.clone());
    document.validate().expect("typed destination validates");

    let value = serde_json::to_value(&document).expect("typed destination serializes");
    assert_eq!(value["kind"], "linkage-map");
    assert_eq!(value["strategy_id"], strategy.id);
    assert_eq!(value["strategy_version"], strategy.version);
    assert_eq!(value["candidate_filter"].as_array().unwrap().len(), 1);
    assert_eq!(value["assertions"].as_array().unwrap().len(), 1);
    assert_eq!(value["content_hash"], strategy.content_hash);
}

#[test]
fn typed_references_enforce_kind_boundaries_and_descendant_scopes() {
    EntityClusterContractSlice {
        profile: typed_reference(EntityContractKind::EntityProfile, "tenant_identity.v1"),
        evidence_policy: typed_reference(
            EntityContractKind::EvidencePolicy,
            "tenant_identity.evidence.v1",
        ),
        frozen_executable_strategy: typed_reference(
            EntityContractKind::FrozenExecutableStrategy,
            "tenant_identity.cluster.exec.v1",
        ),
    }
    .validate()
    .expect("cluster slice validates");

    let wrong_linkage = EntityLinkageContractSlice {
        linkage_map: typed_reference(EntityContractKind::EntityProfile, "oops-profile"),
        evidence_policy: typed_reference(EntityContractKind::EvidencePolicy, "evidence.v1"),
        frozen_executable_strategy: typed_reference(
            EntityContractKind::FrozenExecutableStrategy,
            "link.exec.v1",
        ),
    }
    .validate()
    .expect_err("wrong linkage kind rejects");
    assert_eq!(wrong_linkage.code, EntityTypedContractErrorCode::WrongKind);

    EntityGovernanceContractSlice {
        review_policy: typed_reference(EntityContractKind::ReviewPolicy, "review.v1"),
        promotion_policy: typed_reference(EntityContractKind::PromotionPolicy, "promote.v1"),
    }
    .validate()
    .expect("governance slice validates");

    assert_eq!(
        descendant_labels(EntityContractKind::EntityProfile),
        BTreeSet::from([
            "cluster-lineage".to_string(),
            "promotion-lineage".to_string(),
            "review-lineage".to_string()
        ])
    );
    assert_eq!(
        descendant_labels(EntityContractKind::LinkageMap),
        BTreeSet::from(["linkage-lineage".to_string()])
    );
    assert_eq!(
        descendant_labels(EntityContractKind::ReviewPolicy),
        BTreeSet::from([
            "promotion-lineage".to_string(),
            "review-lineage".to_string()
        ])
    );
}

fn typed_reference(kind: EntityContractKind, id: &str) -> EntityTypedReference {
    EntityTypedReference {
        kind: Some(kind),
        id: id.to_string(),
        version: "2026.07.10".to_string(),
        content_hash: format!("blake3:{id}"),
    }
}

fn descendant_labels(kind: EntityContractKind) -> BTreeSet<String> {
    kind.invalidated_descendants()
        .iter()
        .map(|descendant| {
            serde_json::to_value(descendant)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

fn typed_profile_yaml() -> &'static str {
    r#"
kind: entity-profile
profile: tenant_identity.v1
version: "0.1.0"
entity_type: organization
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields:
  - source_row_id
  - raw_name
normalized_views:
  tenant_core:
    operators:
      - unicode_fold
      - lowercase
      - normalize_whitespace
evidence:
  support:
    - op: exact_view
      view: tenant_core
  cannot_link:
    - op: protected_token_conflict
      view: tenant_core
  relation_hints:
    - op: related_brand_family
      view: tenant_core
patch_namespaces:
  aliases: tenant_identity.v1.aliases
  distinct: tenant_identity.v1.distinct
  relations: tenant_identity.v1.relations
evidence_policy:
  kind: evidence-policy
  id: tenant_identity.evidence.v1
  version: "2026.07.10"
  content_hash: blake3:evidence
review_policy:
  kind: review-policy
  id: tenant_identity.review.v1
  version: "2026.07.10"
  content_hash: blake3:review
promotion_policy:
  kind: promotion-policy
  id: tenant_identity.promote.v1
  version: "2026.07.10"
  content_hash: blake3:promote
frozen_executable_strategy:
  kind: frozen-executable-strategy
  id: tenant_identity.cluster.exec.v1
  version: "2026.07.10"
  content_hash: blake3:frozen
"#
}
