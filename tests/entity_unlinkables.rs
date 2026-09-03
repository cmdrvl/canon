use canon::entity::{
    EntityPatchNamespaces,
    diagnostics::{
        EntityUnlinkablesReport, EntityUnlinkablesReportRequest, EntityUnlinkablesSurfaceInput,
        EntityUnlinkablesSurfaceSide, EntityUnlinkablesThresholds, build_entity_unlinkables_report,
    },
    edge::EdgeEvidenceHit,
    evidence::{ExactViewSupportRequest, exact_view_support_hit},
    prepare::{
        PreparedExactLookup, PreparedExactLookupStatus, PreparedNormalizedView,
        PreparedSurfaceRecord,
    },
    profile::{EntityEvidenceLanes, EntityOperatorSpec, EntityProfileDocument},
    record_link::{
        RecordLinkFeatureKind, RecordLinkFeaturePolicy, RecordLinkFeatureValue,
        RecordLinkSupportPolicy, record_link_self_support_feature,
    },
    run::link::{EntityLinkArtifact, validate_entity_link_artifact_at_path},
    score::{ScoreContribution, ScoreLane, ScoreUnits, accumulate_score_units},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn unlinkables_report_lists_sparse_surface_and_preserves_full_ceiling_parity() {
    let profile = diagnostic_profile();
    let policies = feature_policies();
    let thresholds = EntityUnlinkablesThresholds {
        threshold_source: "test.thresholds".to_string(),
        attach_score_min_units: 9_000,
        backbone_score_min_units: 9_000,
    };
    let full = EntityUnlinkablesSurfaceInput {
        side: EntityUnlinkablesSurfaceSide::Reference,
        surface: surface(
            "surface.full",
            &[
                ("name_key", "north harbor labs"),
                ("name_anchor", "north harbor labs"),
            ],
        ),
        link_ids: vec!["ref-2".to_string(), "ref-1".to_string()],
        record_link_features: full_feature_values(),
        quarantined_record_link_features: BTreeMap::new(),
    };
    let sparse = EntityUnlinkablesSurfaceInput {
        side: EntityUnlinkablesSurfaceSide::Target,
        surface: surface("surface.sparse", &[("name_key", "north harbor labs")]),
        link_ids: vec!["tgt-1".to_string()],
        record_link_features: BTreeMap::new(),
        quarantined_record_link_features: BTreeMap::new(),
    };

    let report = report_for(
        &profile,
        thresholds.clone(),
        vec![sparse.clone(), full.clone()],
        policies.clone(),
    );
    let replay = report_for(&profile, thresholds, vec![full, sparse], policies.clone());

    assert_eq!(
        serde_json::to_vec(&report).expect("report serializes"),
        serde_json::to_vec(&replay).expect("replay report serializes"),
        "row shuffle must not change unlinkables report bytes"
    );
    assert_eq!(report.denominator.subject_surface_count, 2);
    assert_eq!(report.denominator.reference_surface_count, 1);
    assert_eq!(report.denominator.target_surface_count, 1);

    let full_ceiling = report
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "surface.full")
        .expect("full surface reported");
    assert_eq!(full_ceiling.link_ids, vec!["ref-1", "ref-2"]);
    assert_eq!(full_ceiling.max_attainable_support_units, 10_000);
    assert_eq!(full_ceiling.raw_attainable_support_units, 15_000);
    assert!(!full_ceiling.below_attach_threshold);
    assert!(!full_ceiling.below_backbone_threshold);
    assert!(full_ceiling.missing_field_costs.is_empty());
    assert_eq!(
        full_ceiling.score_breakdown,
        expected_full_score_breakdown(&policies)
    );

    let sparse_ceiling = report
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "surface.sparse")
        .expect("sparse surface reported");
    assert_eq!(sparse_ceiling.max_attainable_support_units, 3_000);
    assert!(sparse_ceiling.below_attach_threshold);
    assert!(sparse_ceiling.below_backbone_threshold);
    assert_eq!(
        sparse_ceiling
            .missing_field_costs
            .iter()
            .map(|cost| (cost.field_id.as_str(), cost.cost_units))
            .collect::<Vec<_>>(),
        vec![
            ("name_anchor", 3_000),
            ("amount", 3_000),
            ("category", 3_000),
            ("effective_date", 3_000),
        ]
    );
    assert_eq!(report.unlinkable_surfaces.len(), 1);
    assert_eq!(report.unlinkable_surfaces[0].surface_id, "surface.sparse");
    assert_eq!(
        report.unlinkable_surfaces[0].below_thresholds,
        vec!["attach", "backbone"]
    );
}

#[test]
fn entity_link_artifact_records_unlinkables_section_without_decision_drift() {
    let fixture = LinkFixture::new();
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            path_str(&fixture.reference),
            path_str(&fixture.target),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&fixture.work_dir),
            "--no-witness",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).expect("link output is json");
    let artifact: EntityLinkArtifact =
        serde_json::from_value(payload.clone()).expect("link artifact parses");

    let written_payload: Value = serde_json::from_slice(
        &fs::read(fixture.work_dir.join("link/link.json")).expect("link artifact written"),
    )
    .expect("written link artifact parses");
    assert_eq!(written_payload, payload);
    validate_entity_link_artifact_at_path(&artifact, &fixture.work_dir.join("link/link.json"))
        .expect("link artifact with unlinkables validates");
    assert_eq!(artifact.summary, artifact.decision_artifact.summary);
    let report = artifact
        .unlinkables
        .expect("unlinkables section is present");
    assert_eq!(report.report_type, "unlinkables");
    assert_eq!(
        report.thresholds.threshold_source,
        "strategy.match_threshold"
    );
    assert_eq!(report.thresholds.attach_score_min_units, 7_500);
    assert_eq!(report.thresholds.backbone_score_min_units, 7_500);
    assert_eq!(report.denominator.subject_surface_count, 2);
    assert_eq!(report.denominator.reference_surface_count, 1);
    assert_eq!(report.denominator.target_surface_count, 1);
    assert!(report.unlinkable_surfaces.is_empty());
}

fn report_for(
    profile: &EntityProfileDocument,
    thresholds: EntityUnlinkablesThresholds,
    surfaces: Vec<EntityUnlinkablesSurfaceInput>,
    policies: BTreeMap<String, RecordLinkFeaturePolicy>,
) -> EntityUnlinkablesReport {
    build_entity_unlinkables_report(EntityUnlinkablesReportRequest {
        profile,
        support_namespace: "pkg.synthetic:unlinkables.aliases",
        thresholds,
        surfaces,
        record_link_feature_policies: policies,
    })
    .expect("unlinkables report builds")
}

fn expected_full_score_breakdown(
    policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> canon::entity::score::ScoreBreakdown {
    let mut hits = vec![
        exact_view_hit("name_key", "exact_view:name_key"),
        exact_view_hit("name_anchor", "exact_view:name_anchor"),
    ];
    let features = full_feature_values();
    for (feature_id, value) in &features {
        let comparison = record_link_self_support_feature(feature_id, value, policies)
            .expect("record-link self comparison succeeds")
            .expect("record-link feature supports itself");
        hits.push(EdgeEvidenceHit::new(
            ScoreLane::Support,
            "record_link",
            format!("record_link:{}", comparison.feature_id),
            "record_link_feature_support",
            ScoreUnits::saturating_from_units(comparison.score_units),
            false,
            "record-link feature self-support",
        ));
    }
    accumulate_score_units(hits.iter().map(|hit| {
        ScoreContribution::new(
            hit.lane,
            format!("{}:{}", hit.namespace, hit.operator_id),
            hit.reason_code.clone(),
            hit.score_units,
        )
    }))
}

fn exact_view_hit(view_name: &str, operator_id: &str) -> EdgeEvidenceHit {
    exact_view_support_hit(ExactViewSupportRequest {
        namespace: "pkg.synthetic:unlinkables.aliases",
        operator_id,
        reason_code: "exact_view_support",
        view_name,
        left_value: "north harbor labs",
        right_value: "north harbor labs",
        score_units: ScoreUnits::saturating_from_units(3_000),
    })
    .expect("exact view self-hit")
}

fn diagnostic_profile() -> EntityProfileDocument {
    EntityProfileDocument {
        profile: "pkg.synthetic:unlinkables".to_string(),
        version: "1".to_string(),
        entity_type: "organization".to_string(),
        identity_semantics: "synthetic diagnostic fixture".to_string(),
        canonical_type: "organization".to_string(),
        required_fields: vec!["name".to_string()],
        normalized_views: BTreeMap::new(),
        evidence: EntityEvidenceLanes {
            support: ["name_key", "name_anchor"]
                .into_iter()
                .map(|view| EntityOperatorSpec {
                    op: "exact_view".to_string(),
                    view: Some(view.to_string()),
                    params: BTreeMap::from([("score_units".to_string(), "3000".to_string())]),
                })
                .collect(),
            cannot_link: Vec::new(),
            relation_hints: Vec::new(),
        },
        patch_namespaces: EntityPatchNamespaces {
            aliases: "pkg.synthetic:unlinkables.aliases".to_string(),
            distinct: "pkg.synthetic:unlinkables.distinct".to_string(),
            relations: "pkg.synthetic:unlinkables.relations".to_string(),
        },
    }
}

fn feature_policies() -> BTreeMap<String, RecordLinkFeaturePolicy> {
    BTreeMap::from([
        (
            "amount".to_string(),
            RecordLinkFeaturePolicy {
                feature_id: "amount".to_string(),
                kind: RecordLinkFeatureKind::Numeric,
                support: RecordLinkSupportPolicy::NumericTolerance {
                    tolerance_scaled_units: 0,
                },
                score_units: 3_000,
                hard_conflict_on_mismatch: true,
            },
        ),
        (
            "category".to_string(),
            RecordLinkFeaturePolicy {
                feature_id: "category".to_string(),
                kind: RecordLinkFeatureKind::Categorical,
                support: RecordLinkSupportPolicy::CategoricalExact,
                score_units: 3_000,
                hard_conflict_on_mismatch: true,
            },
        ),
        (
            "effective_date".to_string(),
            RecordLinkFeaturePolicy {
                feature_id: "effective_date".to_string(),
                kind: RecordLinkFeatureKind::Date,
                support: RecordLinkSupportPolicy::DateNear { max_days: 0 },
                score_units: 3_000,
                hard_conflict_on_mismatch: true,
            },
        ),
    ])
}

fn full_feature_values() -> BTreeMap<String, RecordLinkFeatureValue> {
    BTreeMap::from([
        (
            "amount".to_string(),
            RecordLinkFeatureValue::Numeric {
                units: "usd".to_string(),
                scaled_value: 100_00,
                scale: 2,
            },
        ),
        (
            "category".to_string(),
            RecordLinkFeatureValue::Categorical {
                value: "baseline".to_string(),
            },
        ),
        (
            "effective_date".to_string(),
            RecordLinkFeatureValue::Date {
                value: "2026-03-31".to_string(),
            },
        ),
    ])
}

fn surface(id: &str, views: &[(&str, &str)]) -> PreparedSurfaceRecord {
    PreparedSurfaceRecord {
        surface_id: id.to_string(),
        profile_id: "pkg.synthetic:unlinkables".to_string(),
        surface_key: id.to_string(),
        primary_surface: "North Harbor Labs".to_string(),
        normalized_views: views
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_string(),
                    PreparedNormalizedView {
                        value: (*value).to_string(),
                        reason_codes: Vec::new(),
                    },
                )
            })
            .collect(),
        exact_lookup: PreparedExactLookup {
            status: PreparedExactLookupStatus::Unresolved,
            canonical_id: None,
            canonical_type: None,
            rule_id: None,
            matched_input: None,
            lookup_inputs: Vec::new(),
            registry_snapshot: None,
        },
        raw_variants: vec!["North Harbor Labs".to_string()],
        alias_surfaces: Vec::new(),
        mention_surfaces: Vec::new(),
        row_count: 1,
        deal_count: 0,
        provenance_samples: Vec::new(),
    }
}

struct LinkFixture {
    _temp: tempfile::TempDir,
    reference: PathBuf,
    target: PathBuf,
    registry: PathBuf,
    strategy: PathBuf,
    work_dir: PathBuf,
}

impl LinkFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let reference = temp.path().join("reference.csv");
        let target = temp.path().join("target.csv");
        let registry = temp.path().join("registry");
        let strategy = temp.path().join("strategy.yaml");
        let work_dir = temp.path().join("work");
        fs::write(
            &reference,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\nref-1,D1,L1,P1,North Harbor Labs,,[]\n",
        )
        .expect("reference rows");
        fs::write(
            &target,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\ntgt-1,D2,L2,P2,North Harbor Labs LLC,,[]\n",
        )
        .expect("target rows");
        write_registry(&registry);
        fs::write(
            &strategy,
            r#"strategy_id: entity-link-unlinkables-fixture.v1
strategy_version: "1.0.0"
entity_type: tenant_label
description: "Entity link unlinkables integration fixture"
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [loan_id]
candidate_filter: []
assertions:
  - field_ref: mention_surfaces_json
    field_tgt: mention_surfaces_json
    op: exact
    weight: 1.0
    required: true
match_threshold: 0.75
ambiguity_gap: 0.10
max_candidates: 10
"#,
        )
        .expect("strategy");
        Self {
            _temp: temp,
            reference,
            target,
            registry,
            strategy,
            work_dir,
        }
    }
}

fn write_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"entity-link-unlinkables-registry","version":"2026.09.02","description":"entity link unlinkables test registry","updated":"2026-09-02","entry_count":1}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        r#"[
  {"input":"North Harbor Labs","canonical_id":"TNT-NORTH-HARBOR-LABS","canonical_type":"tenant_label","rule_id":"ENTITY_LINK_FIXTURE_INCUMBENT"}
]
"#,
    )
    .expect("aliases");
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}
