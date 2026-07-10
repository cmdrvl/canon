#![forbid(unsafe_code)]

#[path = "../src/providers/reconcile.rs"]
mod reconcile;

use reconcile::{
    CANON_PROVIDER_RECONCILE_VERSION, ConflictPolicy, MissingLinkPolicy, OneToManyPolicy,
    ProviderNativeRecord, ProviderReconcileDocumentationRef, ProviderReconcileErrorCode,
    ProviderReconcileEvidencePolicy, ProviderReconcileFieldMap, ProviderReconcileMapRef,
    ProviderReconcilePackage, ProviderReconcilePackageCompatibility, ProviderReconcileRunInput,
    ReconcileAbstentionKind, ReconcileEvidenceKind, ReconcileFieldComparator,
    ReconcileReviewStateKind, RegistryWritePolicy, StaleLinkPolicy, UnsafeNamespacePolicy,
    UnsafeScopePolicy, UnsafeTypePolicy, canonical_package_bytes, finalize_package,
    package_compatibility, provider_reconcile_package_digest, provider_reconcile_schema_version,
    reconcile_records, resolve_map_ref, simulate_reconciliation_impact,
    validate_package_for_execution,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.provider.reconcile.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/providers/reconcile.rs");

#[test]
fn schema_declares_evidence_only_cross_provider_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");

    assert_eq!(schema["title"], CANON_PROVIDER_RECONCILE_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        provider_reconcile_schema_version()
    );
    assert_eq!(
        schema["x-canon-contract"]["evidence_only"],
        Value::Bool(true)
    );
    assert_eq!(
        schema["x-canon-contract"]["native_namespace_reinterpretation_forbidden"],
        Value::Bool(true)
    );
    assert_eq!(
        schema["x-canon-contract"]["implicit_registry_writes_forbidden"],
        Value::Bool(true)
    );
    assert_eq!(
        schema["x-canon-contract"]["review_states"],
        serde_json::json!([
            "missing_link",
            "one_to_many",
            "stale_link",
            "conflicting_link"
        ])
    );
    assert_eq!(
        schema["x-canon-contract"]["evidence_kinds"],
        serde_json::json!(["proposed_identity", "proposed_relationship", "cannot_link"])
    );
}

#[test]
fn declared_field_maps_emit_typed_evidence_abstentions_and_review_states_deterministically() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = provider_reconcile_package_digest(&package).expect("digest computes");
    let refs = map_refs(&digest);
    let input = sample_input();

    let first = reconcile_records(&package, &refs, &input).expect("first reconciliation");
    let second = reconcile_records(&package, &refs, &input).expect("second reconciliation");

    assert_eq!(canonical_json(&first), canonical_json(&second));
    assert!(first.registry_write_intents.is_empty());

    let impact = simulate_reconciliation_impact(&first);
    assert_eq!(impact.proposed_identity, 2);
    assert_eq!(impact.proposed_relationship, 1);
    assert_eq!(impact.cannot_link, 1);
    assert_eq!(impact.review_states, 4);
    assert_eq!(impact.abstentions, 2);

    let evidence_kinds = first
        .evidence
        .iter()
        .map(|record| record.evidence_kind)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        evidence_kinds,
        BTreeSet::from([
            ReconcileEvidenceKind::ProposedIdentity,
            ReconcileEvidenceKind::ProposedRelationship,
            ReconcileEvidenceKind::CannotLink,
        ])
    );

    let identity = first
        .evidence
        .iter()
        .find(|record| {
            record.evidence_kind == ReconcileEvidenceKind::ProposedIdentity
                && record.match_value == "100"
        })
        .expect("identity evidence");
    assert_eq!(identity.left.native_namespace, "ns.synthetic:alpha");
    assert_eq!(identity.right.native_namespace, "ns.synthetic:beta");
    assert_eq!(identity.left.scope_id, "scope.synthetic:global");
    assert_eq!(identity.right.scope_id, "scope.synthetic:global");

    let relationship = first
        .evidence
        .iter()
        .find(|record| {
            record.evidence_kind == ReconcileEvidenceKind::ProposedRelationship
                && record.match_value == "900"
        })
        .expect("relationship evidence");
    assert_eq!(
        relationship.relationship_term_id.as_deref(),
        Some("pkg.synthetic:parent_of")
    );

    let cannot_link = first
        .evidence
        .iter()
        .find(|record| record.evidence_kind == ReconcileEvidenceKind::CannotLink)
        .expect("cannot-link evidence");
    assert_eq!(cannot_link.match_value, "777");

    assert!(first.review_states.iter().any(|state| {
        state.state_kind == ReconcileReviewStateKind::MissingLink && state.match_value == "404"
    }));
    assert!(first.review_states.iter().any(|state| {
        state.state_kind == ReconcileReviewStateKind::OneToMany && state.match_value == "200"
    }));
    assert!(first.review_states.iter().any(|state| {
        state.state_kind == ReconcileReviewStateKind::StaleLink && state.match_value == "300"
    }));

    let conflict = first
        .review_states
        .iter()
        .find(|state| state.state_kind == ReconcileReviewStateKind::ConflictingLink)
        .expect("conflicting review state");
    assert_eq!(conflict.match_value, "777");
    assert_eq!(conflict.related_evidence_ids.len(), 2);

    assert!(first.abstentions.iter().any(|abstention| {
        abstention.kind == ReconcileAbstentionKind::Type && abstention.match_value == "500"
    }));
    assert!(first.abstentions.iter().any(|abstention| {
        abstention.kind == ReconcileAbstentionKind::Scope && abstention.match_value == "600"
    }));
}

#[test]
fn digest_pinning_and_contract_validation_fail_before_execution() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = provider_reconcile_package_digest(&package).expect("digest computes");
    let refs = map_refs(&digest);

    assert_eq!(
        validate_package_for_execution(&package, &refs).unwrap(),
        digest
    );
    assert_eq!(
        resolve_map_ref(&package, &refs[0]).unwrap().map_id,
        "pkg.synthetic:alpha_beta_identity"
    );

    let mut wrong_refs = refs.clone();
    wrong_refs[0].package_digest = sample_hash('f');
    let error =
        validate_package_for_execution(&package, &wrong_refs).expect_err("digest pinning fails");
    assert_eq!(error.code, ProviderReconcileErrorCode::CompatibilityPolicy);

    let mut invalid = sample_package();
    invalid
        .field_maps
        .iter_mut()
        .find(|map| map.evidence_kind == ReconcileEvidenceKind::ProposedRelationship)
        .expect("relationship map")
        .relationship_term_id = None;
    let error = finalize_package(invalid).expect_err("relationship term is required");
    assert_eq!(error.code, ProviderReconcileErrorCode::ArtifactContract);
}

#[test]
fn package_compatibility_allows_same_major_when_used_maps_still_resolve() {
    let locked = finalize_package(sample_package()).expect("locked package");
    let digest = provider_reconcile_package_digest(&locked).expect("digest computes");
    let refs = map_refs(&digest);

    let mut candidate = sample_package();
    candidate.package_version = "1.1.0".to_string();
    candidate
        .documentation
        .push(ProviderReconcileDocumentationRef {
            label: "operator-notes".to_string(),
            uri: "https://example.invalid/reconcile-notes".to_string(),
        });
    let candidate = finalize_package(candidate).expect("candidate package");

    assert_eq!(
        package_compatibility(&locked, &candidate, &refs).unwrap(),
        ProviderReconcilePackageCompatibility::CompatibleSameMajor
    );
    assert!(!canonical_package_bytes(&locked).unwrap().is_empty());
}

#[test]
fn source_scan_keeps_reconcile_contract_domain_neutral() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();

    for banned in ["openfigi", "sec", "loan", "issuer", "servicer"] {
        assert!(
            !contains_forbidden_word(&lower_source, banned),
            "reconcile module should not embed concrete domain term {banned}"
        );
        assert!(
            !contains_forbidden_word(&lower_schema, banned),
            "reconcile schema should not embed concrete domain term {banned}"
        );
    }
}

fn sample_package() -> ProviderReconcilePackage {
    ProviderReconcilePackage {
        version: provider_reconcile_schema_version().to_string(),
        package_id: "pkg.synthetic.reconcile".to_string(),
        package_version: "1.0.0".to_string(),
        field_maps: vec![
            ProviderReconcileFieldMap {
                map_id: "pkg.synthetic:alpha_beta_identity".to_string(),
                evidence_kind: ReconcileEvidenceKind::ProposedIdentity,
                left_provider_id: "provider.synthetic.alpha".to_string(),
                left_namespace: "ns.synthetic:alpha".to_string(),
                left_scope_id: "scope.synthetic:global".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_provider_id: "provider.synthetic.beta".to_string(),
                right_namespace: "ns.synthetic:beta".to_string(),
                right_scope_id: "scope.synthetic:global".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_field_path: "external_key".to_string(),
                right_field_path: "provider_key".to_string(),
                comparator: ReconcileFieldComparator::Exact,
                relationship_term_id: None,
            },
            ProviderReconcileFieldMap {
                map_id: "pkg.synthetic:alpha_beta_blocked".to_string(),
                evidence_kind: ReconcileEvidenceKind::CannotLink,
                left_provider_id: "provider.synthetic.alpha".to_string(),
                left_namespace: "ns.synthetic:alpha".to_string(),
                left_scope_id: "scope.synthetic:global".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_provider_id: "provider.synthetic.beta".to_string(),
                right_namespace: "ns.synthetic:beta".to_string(),
                right_scope_id: "scope.synthetic:global".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_field_path: "blocked_partner_key".to_string(),
                right_field_path: "provider_key".to_string(),
                comparator: ReconcileFieldComparator::Exact,
                relationship_term_id: None,
            },
            ProviderReconcileFieldMap {
                map_id: "pkg.synthetic:alpha_gamma_parent".to_string(),
                evidence_kind: ReconcileEvidenceKind::ProposedRelationship,
                left_provider_id: "provider.synthetic.alpha".to_string(),
                left_namespace: "ns.synthetic:alpha".to_string(),
                left_scope_id: "scope.synthetic:global".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_provider_id: "provider.synthetic.gamma".to_string(),
                right_namespace: "ns.synthetic:gamma".to_string(),
                right_scope_id: "scope.synthetic:global".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_field_path: "parent_external_key".to_string(),
                right_field_path: "provider_key".to_string(),
                comparator: ReconcileFieldComparator::Exact,
                relationship_term_id: Some("pkg.synthetic:parent_of".to_string()),
            },
        ],
        evidence_policy: ProviderReconcileEvidencePolicy {
            stale_after_days: 365,
            unsafe_namespace_policy: UnsafeNamespacePolicy::Abstain,
            unsafe_scope_policy: UnsafeScopePolicy::Abstain,
            unsafe_type_policy: UnsafeTypePolicy::Abstain,
            missing_link_policy: MissingLinkPolicy::Review,
            one_to_many_policy: OneToManyPolicy::Review,
            stale_link_policy: StaleLinkPolicy::Review,
            conflict_policy: ConflictPolicy::Review,
            registry_write_policy: RegistryWritePolicy::NeverImplicit,
        },
        documentation: vec![ProviderReconcileDocumentationRef {
            label: "contract".to_string(),
            uri: "https://example.invalid/provider-reconcile".to_string(),
        }],
    }
}

fn sample_input() -> ProviderReconcileRunInput {
    ProviderReconcileRunInput {
        as_of: "2026-07-10T00:00:00Z".to_string(),
        left_records: vec![
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-100",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[
                    ("external_key", "100"),
                    ("parent_external_key", "900"),
                    ("display_name", "Northwind Collective"),
                ],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-200",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[("external_key", "200"), ("display_name", "Altair Fabric")],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-300",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[("external_key", "300"), ("display_name", "Stale Match")],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-404",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[("external_key", "404"), ("display_name", "Missing Match")],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-500",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[("external_key", "500"), ("display_name", "Type Clash")],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-600",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[("external_key", "600"), ("display_name", "Scope Clash")],
            ),
            record(
                "provider.synthetic.alpha",
                "ns.synthetic:alpha",
                "alpha-777",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-01T00:00:00Z",
                &[
                    ("external_key", "777"),
                    ("blocked_partner_key", "777"),
                    ("display_name", "Contradictory Alias"),
                ],
            ),
        ],
        right_records: vec![
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-100",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-02T00:00:00Z",
                &[
                    ("provider_key", "100"),
                    ("display_name", "Northwind Systems"),
                ],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-200-a",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-02T00:00:00Z",
                &[("provider_key", "200"), ("display_name", "Altair West")],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-200-b",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-02T00:00:00Z",
                &[("provider_key", "200"), ("display_name", "Altair East")],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-300",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2024-01-01T00:00:00Z",
                &[
                    ("provider_key", "300"),
                    ("display_name", "Stale Match Renamed"),
                ],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-500",
                "scope.synthetic:global",
                "types.synthetic:site",
                "2026-07-02T00:00:00Z",
                &[("provider_key", "500"), ("display_name", "Scope Tower")],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-600",
                "scope.synthetic:regional",
                "types.synthetic:organization",
                "2026-07-02T00:00:00Z",
                &[("provider_key", "600"), ("display_name", "Regional Ledger")],
            ),
            record(
                "provider.synthetic.beta",
                "ns.synthetic:beta",
                "beta-777",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-02T00:00:00Z",
                &[("provider_key", "777"), ("display_name", "Conflict Pair")],
            ),
            record(
                "provider.synthetic.gamma",
                "ns.synthetic:gamma",
                "gamma-900",
                "scope.synthetic:global",
                "types.synthetic:organization",
                "2026-07-03T00:00:00Z",
                &[
                    ("provider_key", "900"),
                    ("display_name", "Northwind Parent"),
                ],
            ),
        ],
    }
}

fn map_refs(digest: &str) -> Vec<ProviderReconcileMapRef> {
    vec![
        ProviderReconcileMapRef {
            package_digest: digest.to_string(),
            map_id: "pkg.synthetic:alpha_beta_identity".to_string(),
        },
        ProviderReconcileMapRef {
            package_digest: digest.to_string(),
            map_id: "pkg.synthetic:alpha_beta_blocked".to_string(),
        },
        ProviderReconcileMapRef {
            package_digest: digest.to_string(),
            map_id: "pkg.synthetic:alpha_gamma_parent".to_string(),
        },
    ]
}

fn record(
    provider_id: &str,
    native_namespace: &str,
    native_id: &str,
    scope_id: &str,
    object_type_ref: &str,
    observed_at: &str,
    fields: &[(&str, &str)],
) -> ProviderNativeRecord {
    ProviderNativeRecord {
        provider_id: provider_id.to_string(),
        native_namespace: native_namespace.to_string(),
        native_id: native_id.to_string(),
        scope_id: scope_id.to_string(),
        object_type_ref: object_type_ref.to_string(),
        observed_at: observed_at.to_string(),
        fields: fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn sample_hash(seed: char) -> String {
    let hex = seed.to_string().repeat(64);
    format!("blake3:{hex}")
}

fn canonical_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("value serializes")
}

fn contains_forbidden_word(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == needle)
}
