#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::entity::{
    EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
    EntityRegistrySnapshot, EntityStrategyReference,
    review::{ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem},
    review_export::{
        CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION, NativeReviewExportRequest,
        build_native_review_artifact,
    },
    review_import::{
        NativeReviewDecision, NativeReviewDecisionAction, NativeReviewDecisionContext,
        NativeReviewDecisionMode,
    },
    score::ScoreUnits,
    solve::{SolveEvidenceCut, SolveReconciliationState},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

const DOC: &str = include_str!("../docs/EXTERNAL_CANDIDATE_IMPORT.md");
const CORPUS_FIXTURE: &str = "tests/fixtures/entity/external_import/bdc_investments.csv";
const MODEL_FIXTURE: &str = "tests/fixtures/entity/external_import/splink_saved_model.json";
const DECISIONS_FIXTURE: &str = "tests/fixtures/entity/external_import/pre_scored_decisions.json";
const REGISTRY_JSON_FIXTURE: &str = "tests/fixtures/entity/external_import/registry/registry.json";
const ALIASES_JSON_FIXTURE: &str = "tests/fixtures/entity/external_import/registry/aliases.json";

#[test]
fn external_candidate_fixture_imports_through_native_review_path() {
    assert_disposable_bridge_documented();
    let scored = scored_fixture();
    assert_eq!(corpus_rows().len(), 4, "fixture denominator is predeclared");
    assert_eq!(
        scored.decisions.len(),
        3,
        "fixture decision denominator is predeclared"
    );

    let fixture = materialize_import_fixture(&scored, false);
    let registry_before = registry_snapshot(&fixture.registry);
    let output = canon_review_import(&fixture)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let receipt: Value = serde_json::from_slice(&output).expect("native import receipt");

    assert_eq!(receipt["version"], "canon_entity_native_review_import.v0");
    assert_eq!(receipt["accepted_decisions"], 3);
    assert_eq!(
        receipt["source_review_artifact_hash"],
        fixture.source_review["artifact_content_hash"]
    );
    assert_eq!(
        receipt["source_review_queue_hash"],
        fixture.source_review["binding"]["source_review_queue_hash"]
    );
    assert_eq!(
        receipt["patches"]["alias_patches"]
            .as_array()
            .expect("alias patches")
            .len(),
        1
    );
    assert_eq!(
        receipt["patches"]["cannot_link_patches"]
            .as_array()
            .expect("cannot-link patches")
            .len(),
        1
    );
    assert_eq!(
        receipt["patches"]["defer_patches"]
            .as_array()
            .expect("defer patches")
            .len(),
        1
    );
    assert_eq!(
        receipt["strategy_hash"],
        fixture.source_review["binding"]["strategy_hash"]
    );
    assert_eq!(
        registry_snapshot(&fixture.registry),
        registry_before,
        "native source-review import must not mutate registry files"
    );
}

#[test]
fn external_candidate_conflict_refuses_atomically() {
    let scored = scored_fixture();
    let fixture = materialize_import_fixture(&scored, true);
    let registry_before = registry_snapshot(&fixture.registry);
    let output = canon_review_import(&fixture)
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&output).expect("refusal json");

    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_REVIEW_IMPORT");
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "identity_cannot_link_conflict"
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(
        registry_snapshot(&fixture.registry),
        registry_before,
        "conflicting externally-authored decisions must not mutate registry files"
    );
}

fn assert_disposable_bridge_documented() {
    for required in [
        "estimate_u(seed=...)",
        "save_model_to_json",
        "compare_records",
        "not designed for a single bag-of-words column",
        "bd-3qfq",
        "bd-3el3",
        "bd-2qgz",
        "does not read or mutate the registry",
    ] {
        assert!(
            DOC.contains(required),
            "external candidate import doc is missing {required}"
        );
    }
}

fn canon_review_import(fixture: &ImportFixture) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.args([
        "entity",
        "review",
        "import",
        fixture.decisions_path.to_str().expect("decisions path"),
        "--registry",
        fixture.registry.to_str().expect("registry path"),
        "--next-version",
        "2026.09.03-external-import",
        "--source-review",
        fixture
            .source_review_path
            .to_str()
            .expect("source review path"),
        "--emit",
        "json",
    ]);
    command
}

fn materialize_import_fixture(scored: &ExternalDecisionSet, conflict: bool) -> ImportFixture {
    let temp_dir = tempdir().expect("temp dir");
    let registry = copy_registry(temp_dir.path());
    let source_review = source_review_artifact(scored, conflict);
    let source_review_path = temp_dir.path().join("native-review.json");
    fs::write(
        &source_review_path,
        serde_json::to_vec_pretty(&source_review).expect("source review json"),
    )
    .expect("write source review");
    let decisions = if conflict {
        conflict_decisions_from_artifact(&source_review, scored)
    } else {
        native_decisions_from_artifact(&source_review, &scored.decisions)
    };
    let decisions_path = temp_dir.path().join("external-native-decisions.json");
    fs::write(
        &decisions_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION,
            "decisions": decisions,
        }))
        .expect("decision json"),
    )
    .expect("write decisions");
    ImportFixture {
        _temp_dir: temp_dir,
        registry,
        source_review_path,
        decisions_path,
        source_review,
    }
}

fn source_review_artifact(scored: &ExternalDecisionSet, conflict: bool) -> Value {
    let mut decisions = scored.decisions.clone();
    if conflict {
        decisions.push(scored.negative_conflict_decision.clone());
    }
    let review_queue = ReviewQueueArtifact {
        version: "canon_entity_review_queue.v0".to_string(),
        artifact_content_hash: hash_labeled_files(&[
            ("corpus", fixture_path(CORPUS_FIXTURE)),
            ("model", fixture_path(MODEL_FIXTURE)),
            ("external_decisions", fixture_path(DECISIONS_FIXTURE)),
        ]),
        metadata: metadata(),
        summary: canon::entity::EntityDeterministicSummary::default(),
        source_solve_hash: "blake3:external-candidate-source-solve".to_string(),
        source_link_hash: None,
        review_items: decisions
            .iter()
            .map(|decision| review_item(decision, scored))
            .collect(),
    };
    let artifact = build_native_review_artifact(NativeReviewExportRequest {
        review_queue,
        run_content_hash: hash_labeled_files(&[
            ("corpus", fixture_path(CORPUS_FIXTURE)),
            ("native_review_run", fixture_path(DECISIONS_FIXTURE)),
        ]),
        policy_content_hash: hash_labeled_files(&[
            ("model", fixture_path(MODEL_FIXTURE)),
            ("threshold", fixture_path(DECISIONS_FIXTURE)),
        ]),
    })
    .expect("native review artifact builds");
    serde_json::to_value(artifact).expect("native review value")
}

fn review_item(decision: &ExternalDecision, scored: &ExternalDecisionSet) -> ReviewQueueItem {
    assert!(
        !decision.why.trim().is_empty(),
        "fixture must explain why each external decision was authored"
    );
    let rows = corpus_rows_by_surface_id();
    let surfaces = sorted(decision.surface_ids.clone());
    let positive = matches!(
        decision.action,
        ExternalAction::Match | ExternalAction::Distinct
    );
    let negative = decision.native_cannot_link_cue;
    ReviewQueueItem {
        review_id: decision.review_id.clone(),
        ambiguity_key: format!("external-candidate:{}", decision.review_id),
        component_id: format!("component:{}", decision.review_id.replace(':', "_")),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_external_candidate_decision".to_string(),
        review_priority_units: review_priority_units(decision),
        priority_reasons: priority_reasons(decision),
        affected_rows: surfaces.len() as u64,
        affected_deals: 1,
        surface_ids: surfaces.clone(),
        strongest_positive_cut: positive.then(|| {
            evidence_cut(
                &surfaces[0],
                &surfaces[1],
                decision.external_probability_microunits / 100,
                "external_seeded_score",
            )
        }),
        strongest_negative_cut: negative.then(|| {
            evidence_cut(
                &surfaces[0],
                &surfaces[1],
                9300,
                "native_amount_date_category_conflict",
            )
        }),
        relation_hints: Vec::new(),
        provenance_samples: surfaces
            .iter()
            .map(|surface_id| {
                let row = rows
                    .get(surface_id)
                    .unwrap_or_else(|| panic!("surface {surface_id} fixture row"));
                provenance(surface_id, row, scored)
            })
            .collect(),
    }
}

fn native_decisions_from_artifact(
    source_review: &Value,
    external_decisions: &[ExternalDecision],
) -> Vec<NativeReviewDecision> {
    external_decisions
        .iter()
        .map(|decision| native_decision_from_artifact(source_review, decision))
        .collect()
}

fn conflict_decisions_from_artifact(
    source_review: &Value,
    scored: &ExternalDecisionSet,
) -> Vec<NativeReviewDecision> {
    let hard_negative = scored
        .decisions
        .iter()
        .find(|decision| decision.review_id == "review:bdc-acme-distinct")
        .expect("hard negative decision");
    vec![
        native_decision_from_artifact(source_review, hard_negative),
        native_decision_from_artifact(source_review, &scored.negative_conflict_decision),
    ]
}

fn native_decision_from_artifact(
    source_review: &Value,
    external: &ExternalDecision,
) -> NativeReviewDecision {
    let item = source_review["review_items"]
        .as_array()
        .expect("review items")
        .iter()
        .find(|item| item["review_id"].as_str() == Some(external.review_id.as_str()))
        .unwrap_or_else(|| panic!("review item {} exists", external.review_id));
    let binding = &source_review["binding"];
    NativeReviewDecision {
        review_id: external.review_id.clone(),
        mode: match item["mode"].as_str().expect("mode") {
            "cluster" => NativeReviewDecisionMode::Cluster,
            other => panic!("unexpected mode {other}"),
        },
        action: match external.action {
            ExternalAction::Match => NativeReviewDecisionAction::Alias,
            ExternalAction::Distinct => NativeReviewDecisionAction::CannotLink,
            ExternalAction::Defer => NativeReviewDecisionAction::Defer,
        },
        operator_id: "operator:external-splink-fixture".to_string(),
        reason_code: external.reason_code.clone(),
        note: format!(
            "seed={} probability_microunits={}",
            binding["strategy_hash"].as_str().expect("strategy hash"),
            external.external_probability_microunits
        ),
        source_review_artifact_hash: source_review["artifact_content_hash"]
            .as_str()
            .expect("artifact hash")
            .to_string(),
        decision_binding_hash: item["decision_binding_hash"]
            .as_str()
            .expect("decision binding")
            .to_string(),
        run_content_hash: binding["run_content_hash"]
            .as_str()
            .expect("run hash")
            .to_string(),
        policy_content_hash: binding["policy_content_hash"]
            .as_str()
            .expect("policy hash")
            .to_string(),
        registry_snapshot_hash: binding["registry_snapshot_hash"]
            .as_str()
            .expect("registry hash")
            .to_string(),
        mode_context: serde_json::from_value(item["mode_context"].clone()).expect("mode context"),
        surface_ids: Vec::new(),
        target_canonical_id: external.target_canonical_id.clone(),
        relation: None,
    }
}

fn metadata() -> EntityArtifactMetadata {
    let namespace = "bdc_investment_identity";
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: namespace.to_string(),
            version: "0.1.0".to_string(),
            entity_type: "bdc_investment".to_string(),
            identity_semantics: "issuer_coupon_maturity_type".to_string(),
            canonical_type: "investment_position".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: format!("{namespace}.aliases"),
                distinct: format!("{namespace}.distinct"),
                relations: format!("{namespace}.relations"),
            },
            content_hash: Some(hash_labeled_files(&[
                ("corpus", fixture_path(CORPUS_FIXTURE)),
                ("model", fixture_path(MODEL_FIXTURE)),
            ])),
        },
        strategy: EntityStrategyReference {
            id: "bdc.external_candidate_bridge.v0".to_string(),
            version: "0.1.0".to_string(),
            content_hash: hash_labeled_files(&[
                ("model", fixture_path(MODEL_FIXTURE)),
                ("selection_seed", fixture_path(DECISIONS_FIXTURE)),
            ]),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "bdc-investments".to_string(),
            version: "2026.09.02".to_string(),
            source: "tests/fixtures/entity/external_import/registry".to_string(),
            lookup_snapshot_hash: registry_fixture_hash(),
            sidecar_snapshot_hash: Some(hash_labeled_files(&[(
                "external_decisions",
                fixture_path(DECISIONS_FIXTURE),
            )])),
        },
        patch_namespace: format!("{namespace}.aliases"),
        input: Some(EntityInputReference {
            row_count: corpus_rows().len() as u64,
            content_hash: hash_labeled_files(&[("corpus", fixture_path(CORPUS_FIXTURE))]),
        }),
        upstream_artifacts: Vec::new(),
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn priority_reasons(decision: &ExternalDecision) -> Vec<String> {
    let mut reasons = vec![
        "external_seeded_candidate".to_string(),
        decision.reason_code.clone(),
    ];
    if decision.native_cannot_link_cue {
        reasons.push("native_cannot_link_cue".to_string());
    }
    reasons.push(decision.action.as_str().to_string());
    reasons
}

fn review_priority_units(decision: &ExternalDecision) -> u32 {
    match decision.action {
        ExternalAction::Match | ExternalAction::Distinct => {
            decision.external_probability_microunits / 100
        }
        ExternalAction::Defer => 0,
    }
}

fn evidence_cut(
    left_surface_id: &str,
    right_surface_id: &str,
    score_units: u32,
    reason_code: &str,
) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        score_units: ScoreUnits::saturating_from_units(score_units.into()),
        evidence_count: 1,
        evidence_reason_codes: vec![reason_code.to_string()],
        evidence_hits: Vec::new(),
    }
}

fn provenance(
    surface_id: &str,
    row: &BdcInvestmentRow,
    scored: &ExternalDecisionSet,
) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row.row_id.clone(),
        source: scored.source_corpus_path.clone(),
        raw_value: format!(
            "{}|{}|{}|{}",
            row.issuer, row.coupon_basis_points, row.maturity_date, row.instrument_type
        ),
    }
}

fn scored_fixture() -> ExternalDecisionSet {
    let bytes = fs::read(fixture_path(DECISIONS_FIXTURE)).expect("external decision fixture");
    let scored: ExternalDecisionSet =
        serde_json::from_slice(&bytes).expect("external decisions parse");
    assert_eq!(scored.version, "external_fixture.candidate_decisions.v0");
    assert_eq!(scored.selection_seed, "bd-2hjs-seed-20260902");
    assert_eq!(scored.source_corpus_path, CORPUS_FIXTURE);
    assert_eq!(scored.source_model_path, MODEL_FIXTURE);
    assert_eq!(scored.operator_probability_threshold_microunits, 970000);
    scored
}

fn corpus_rows() -> Vec<BdcInvestmentRow> {
    let mut reader =
        csv::Reader::from_path(fixture_path(CORPUS_FIXTURE)).expect("corpus csv opens");
    reader
        .deserialize::<BdcInvestmentRow>()
        .collect::<Result<Vec<_>, _>>()
        .expect("corpus csv parses")
}

fn corpus_rows_by_surface_id() -> BTreeMap<String, BdcInvestmentRow> {
    corpus_rows()
        .into_iter()
        .map(|row| (row.surface_id.clone(), row))
        .collect()
}

fn copy_registry(root: &Path) -> PathBuf {
    let registry = root.join("registry");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::copy(
        fixture_path(REGISTRY_JSON_FIXTURE),
        registry.join("registry.json"),
    )
    .expect("copy registry json");
    fs::copy(
        fixture_path(ALIASES_JSON_FIXTURE),
        registry.join("aliases.json"),
    )
    .expect("copy aliases json");
    registry
}

fn registry_snapshot(registry: &Path) -> BTreeMap<String, Vec<u8>> {
    ["registry.json", "aliases.json"]
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                fs::read(registry.join(name)).expect("registry snapshot file"),
            )
        })
        .collect()
}

fn registry_fixture_hash() -> String {
    hash_labeled_files(&[
        ("registry_json", fixture_path(REGISTRY_JSON_FIXTURE)),
        ("aliases_json", fixture_path(ALIASES_JSON_FIXTURE)),
    ])
}

fn hash_labeled_files(parts: &[(&str, PathBuf)]) -> String {
    let mut hasher = blake3::Hasher::new();
    for (label, path) in parts {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!("read hash input {} at {}: {error}", label, path.display())
        });
        hasher.update(label.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize())
}

fn fixture_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

#[derive(Debug)]
struct ImportFixture {
    _temp_dir: TempDir,
    registry: PathBuf,
    source_review_path: PathBuf,
    decisions_path: PathBuf,
    source_review: Value,
}

#[derive(Debug, Deserialize)]
struct ExternalDecisionSet {
    version: String,
    selection_seed: String,
    source_corpus_path: String,
    source_model_path: String,
    operator_probability_threshold_microunits: u32,
    decisions: Vec<ExternalDecision>,
    negative_conflict_decision: ExternalDecision,
}

#[derive(Debug, Clone, Deserialize)]
struct ExternalDecision {
    review_id: String,
    surface_ids: Vec<String>,
    external_probability_microunits: u32,
    native_cannot_link_cue: bool,
    action: ExternalAction,
    reason_code: String,
    target_canonical_id: Option<String>,
    why: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExternalAction {
    Match,
    Distinct,
    Defer,
}

impl ExternalAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Match => "external_match",
            Self::Distinct => "native_distinct",
            Self::Defer => "external_defer",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BdcInvestmentRow {
    row_id: String,
    surface_id: String,
    issuer: String,
    coupon_basis_points: u32,
    maturity_date: String,
    instrument_type: String,
}
