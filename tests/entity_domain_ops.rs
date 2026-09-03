#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord},
    prepare::PreparedSurfaceRecord,
    profile_package::{
        EntityEvidenceLanes, EntityNormalizationOperatorKind, EntityNormalizationOperatorSpec,
        EntityNormalizedView, EntityOperatorSpec, EntityPatchNamespaces, EntityProfileFieldMapping,
        EntityProfileLimits, EntityProfileMode, EntityProfilePackage, LinkDirection,
        ProfileCapability, ProfileModeKind, ProfilePackageRef, ProfilePackageRefKind,
        canonical_package_bytes,
    },
    run::{EntityRunRequest, run_entity_workbench},
    score::ScoreLane,
};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const OBJECT_TYPE: &str = "pkg.synthetic:organization";
const PROFILE_ID: &str = "pkg.synthetic.domain_ops";

#[test]
fn configured_domain_ops_emit_distinct_support_and_reject_runtime_negatives() {
    let fixture = DomainOpsFixture::new();
    let rows = fixture.write_rows(
        "domain_ops.csv",
        &[
            ("row-1", "SMITH JOHN", "SMITH JOHN", "2031-01-12"),
            ("row-2", "JOHN SMITH", "JOHN SMITH", "2031-01-21"),
            (
                "row-3",
                "JOHN SMITH ROBERT",
                "JOHN SMITH ROBERT",
                "2031-02-12",
            ),
        ],
    );
    let profile = fixture.write_profile("configured_profile.json", true);
    let work_dir = fixture.path("configured_work");

    run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: &profile,
        registry: &fixture.registry,
        work_dir: &work_dir,
    })
    .expect("domain ops run succeeds");

    let evidence: Vec<EdgeEvidenceRecord> = read_jsonl(&work_dir.join("evidence/evidence.jsonl"));
    let cores = surface_cores(&work_dir);
    let positive = record_for_cores(&evidence, &cores, "smith john", "john smith");
    let date_hit = support_hit(positive, "date_transposed_digits:maturity_date")
        .expect("transposed date support hit");
    assert_eq!(date_hit.reason_code, "transposed_digit_date_support");
    assert_eq!(date_hit.score_units.as_u32(), 8_250);
    assert!(
        date_hit
            .explanation
            .contains("adjacent digit transposition"),
        "date explanation labels the transposition: {}",
        date_hit.explanation
    );
    assert!(
        date_hit.explanation.contains("damerau_score_units=9000"),
        "date explanation records the Damerau score: {}",
        date_hit.explanation
    );

    let reversal_hit = support_hit(positive, "two_token_reversal:name_reversal")
        .expect("two-token reversal support hit");
    assert_eq!(reversal_hit.reason_code, "two_token_reversal_support");
    assert_eq!(reversal_hit.score_units.as_u32(), 7_500);

    let month_change = record_for_cores(&evidence, &cores, "smith john", "john smith robert");
    assert!(
        support_hit(month_change, "date_transposed_digits:maturity_date").is_none(),
        "real month change must not be labeled as a digit transposition"
    );

    let partial_reversal_records = evidence
        .iter()
        .filter(|record| record_has_core(record, &cores, "john smith robert"))
        .collect::<Vec<_>>();
    assert!(
        !partial_reversal_records.is_empty(),
        "fixture must produce at least one candidate involving the partial three-token reversal"
    );
    assert!(
        partial_reversal_records.iter().all(|record| support_hit(
            record,
            "two_token_reversal:name_reversal"
        )
        .is_none()),
        "partial three-token reversal must not emit reversal support"
    );
}

#[test]
fn domain_ops_are_configured_noop_absent_and_deterministic_under_shuffle() {
    let rows = [
        ("row-1", "SMITH JOHN", "SMITH JOHN", "2031-01-12"),
        ("row-2", "JOHN SMITH", "JOHN SMITH", "2031-01-21"),
        (
            "row-3",
            "JOHN SMITH ROBERT",
            "JOHN SMITH ROBERT",
            "2031-02-12",
        ),
    ];
    let shuffled_rows = [rows[2], rows[0], rows[1]];
    let fixture = DomainOpsFixture::new();
    let profile = fixture.write_profile("configured_profile.json", true);
    let first_rows = fixture.write_rows("first.csv", &rows);
    let shuffled = fixture.write_rows("shuffled.csv", &shuffled_rows);
    let first_work = fixture.path("first_work");
    let shuffled_work = fixture.path("shuffled_work");

    run_fixture(&fixture, &first_rows, &profile, &first_work);
    run_fixture(&fixture, &shuffled, &profile, &shuffled_work);
    assert_eq!(
        fs::read(first_work.join("evidence/evidence.jsonl")).expect("first evidence bytes"),
        fs::read(shuffled_work.join("evidence/evidence.jsonl")).expect("shuffled evidence bytes"),
        "configured domain ops must produce byte-identical evidence under row shuffle"
    );

    let noop_profile = fixture.write_profile("noop_profile.json", false);
    let noop_work = fixture.path("noop_work");
    run_fixture(&fixture, &first_rows, &noop_profile, &noop_work);
    let noop_evidence: Vec<EdgeEvidenceRecord> =
        read_jsonl(&noop_work.join("evidence/evidence.jsonl"));
    assert!(
        noop_evidence
            .iter()
            .flat_map(|record| &record.hits)
            .all(|hit| {
                hit.operator_id != "date_transposed_digits:maturity_date"
                    && hit.operator_id != "two_token_reversal:name_reversal"
                    && hit.reason_code != "transposed_digit_date_support"
                    && hit.reason_code != "two_token_reversal_support"
            }),
        "absent-config profile must not emit the new support operators"
    );
}

struct DomainOpsFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    registry: PathBuf,
}

impl DomainOpsFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let registry = root.join("registry");
        fs::create_dir_all(&registry).expect("registry dir");
        fs::write(
            registry.join("registry.json"),
            r#"{"id":"domain-ops","version":"2026.09.03","description":"Domain ops test registry","updated":"2026-09-03","entry_count":0}"#,
        )
        .expect("registry metadata");
        fs::write(registry.join("aliases.json"), "[]").expect("empty aliases");
        Self {
            _temp: temp,
            root,
            registry,
        }
    }

    fn path(&self, relpath: &str) -> PathBuf {
        self.root.join(relpath)
    }

    fn write_rows(&self, name: &str, rows: &[(&str, &str, &str, &str)]) -> PathBuf {
        let path = self.path(name);
        let mut csv = String::from("observation_id,raw_name,raw_name_reversal,maturity_date\n");
        for (observation_id, raw_name, raw_name_reversal, maturity_date) in rows {
            csv.push_str(observation_id);
            csv.push(',');
            csv.push_str(raw_name);
            csv.push(',');
            csv.push_str(raw_name_reversal);
            csv.push(',');
            csv.push_str(maturity_date);
            csv.push('\n');
        }
        fs::write(&path, csv).expect("rows csv");
        path
    }

    fn write_profile(&self, name: &str, configured: bool) -> PathBuf {
        let path = self.path(name);
        let bytes = canonical_package_bytes(&profile_package(configured))
            .expect("profile package canonicalizes");
        fs::write(&path, bytes).expect("profile package json");
        path
    }
}

fn run_fixture(fixture: &DomainOpsFixture, rows: &Path, profile: &Path, work_dir: &Path) {
    run_entity_workbench(EntityRunRequest {
        rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: profile,
        registry: &fixture.registry,
        work_dir,
    })
    .expect("domain ops run succeeds");
}

fn profile_package(configured: bool) -> EntityProfilePackage {
    EntityProfilePackage {
        kind: "entity-profile".to_string(),
        profile: PROFILE_ID.to_string(),
        version: "1.0.0".to_string(),
        entity_type: OBJECT_TYPE.to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        canonical_type: "organization_id".to_string(),
        required_fields: vec![
            "observation_id".to_string(),
            "raw_name".to_string(),
            "raw_name_reversal".to_string(),
            "maturity_date".to_string(),
        ],
        normalized_views: BTreeMap::from([
            (
                "core".to_string(),
                view(vec![
                    EntityNormalizationOperatorKind::Lowercase,
                    EntityNormalizationOperatorKind::NormalizeWhitespace,
                ]),
            ),
            (
                "name_reversal".to_string(),
                view(vec![
                    EntityNormalizationOperatorKind::Lowercase,
                    EntityNormalizationOperatorKind::NormalizeWhitespace,
                    EntityNormalizationOperatorKind::ReverseTwoTokens,
                ]),
            ),
            (
                "maturity_date".to_string(),
                view(vec![EntityNormalizationOperatorKind::AsciiTrimUpper]),
            ),
        ]),
        evidence: EntityEvidenceLanes {
            support: support_ops(configured),
            cannot_link: vec![EntityOperatorSpec {
                op: "protected_token_conflict".to_string(),
                view: Some("core".to_string()),
                params: BTreeMap::new(),
            }],
            relation_hints: vec![EntityOperatorSpec {
                op: "context_alignment".to_string(),
                view: Some("core".to_string()),
                params: BTreeMap::new(),
            }],
        },
        patch_namespaces: EntityPatchNamespaces {
            aliases: format!("{PROFILE_ID}.aliases"),
            distinct: format!("{PROFILE_ID}.distinct"),
            relations: format!("{PROFILE_ID}.relations"),
        },
        evidence_policy: package_ref(
            ProfilePackageRefKind::EvidencePolicy,
            "evidence_policy",
            'a',
        ),
        review_policy: package_ref(ProfilePackageRefKind::ReviewPolicy, "review_policy", 'b'),
        promotion_policy: package_ref(
            ProfilePackageRefKind::PromotionPolicy,
            "promotion_policy",
            'c',
        ),
        frozen_executable_strategy: package_ref(
            ProfilePackageRefKind::FrozenExecutableStrategy,
            "strategy",
            'd',
        ),
        ontology_package: package_ref(ProfilePackageRefKind::OntologyPackage, "ontology", 'e'),
        identifier_package: package_ref(
            ProfilePackageRefKind::IdentifierPackage,
            "identifier",
            'f',
        ),
        vocabulary_package: package_ref(
            ProfilePackageRefKind::VocabularyPackage,
            "vocabulary",
            '1',
        ),
        evidence_package: package_ref(ProfilePackageRefKind::EvidencePackage, "evidence", '2'),
        normalization_packages: vec![package_ref(
            ProfilePackageRefKind::NormalizationPackage,
            "normalization",
            '3',
        )],
        available_capabilities: vec![
            ProfileCapability::Prepare,
            ProfileCapability::Index,
            ProfileCapability::Block,
            ProfileCapability::Evidence,
            ProfileCapability::SolveCluster,
            ProfileCapability::Review,
            ProfileCapability::Promote,
            ProfileCapability::Apply,
        ],
        field_mappings: vec![
            field_mapping("observation_id", "record_key", None, true),
            field_mapping("raw_name", "canonical_surface", Some("core"), true),
            field_mapping(
                "raw_name_reversal",
                "context_value",
                Some("name_reversal"),
                true,
            ),
            field_mapping(
                "maturity_date",
                "context_value",
                Some("maturity_date"),
                true,
            ),
        ],
        execution_modes: vec![EntityProfileMode {
            mode: ProfileModeKind::Cluster,
            source_object_type: OBJECT_TYPE.to_string(),
            target_object_type: None,
            link_direction: None::<LinkDirection>,
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Index,
                ProfileCapability::Block,
                ProfileCapability::Evidence,
                ProfileCapability::SolveCluster,
                ProfileCapability::Review,
                ProfileCapability::Promote,
                ProfileCapability::Apply,
            ],
            field_paths: vec![
                "observation_id".to_string(),
                "raw_name".to_string(),
                "raw_name_reversal".to_string(),
                "maturity_date".to_string(),
            ],
            outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        }],
        limits: EntityProfileLimits {
            max_observation_fields: 8,
            max_candidate_pairs: 200,
            max_outputs: 20,
        },
        expected_outputs: vec![
            "prepare_bundle".to_string(),
            "cluster_assignments".to_string(),
            "review_queue".to_string(),
        ],
        project_overrides: Vec::new(),
    }
}

fn support_ops(configured: bool) -> Vec<EntityOperatorSpec> {
    if configured {
        vec![
            EntityOperatorSpec {
                op: "date_transposed_digits".to_string(),
                view: Some("maturity_date".to_string()),
                params: BTreeMap::from([("score_units".to_string(), "8250".to_string())]),
            },
            EntityOperatorSpec {
                op: "two_token_reversal".to_string(),
                view: Some("name_reversal".to_string()),
                params: BTreeMap::from([("score_units".to_string(), "7500".to_string())]),
            },
        ]
    } else {
        vec![EntityOperatorSpec {
            op: "exact_view".to_string(),
            view: Some("core".to_string()),
            params: BTreeMap::from([("score_units".to_string(), "0".to_string())]),
        }]
    }
}

fn view(ops: Vec<EntityNormalizationOperatorKind>) -> EntityNormalizedView {
    EntityNormalizedView {
        operators: ops
            .into_iter()
            .map(|op| EntityNormalizationOperatorSpec {
                op,
                params: BTreeMap::new(),
            })
            .collect(),
    }
}

fn field_mapping(
    field_path: &str,
    field_role: &str,
    normalized_view: Option<&str>,
    required: bool,
) -> EntityProfileFieldMapping {
    EntityProfileFieldMapping {
        field_path: field_path.to_string(),
        object_type: OBJECT_TYPE.to_string(),
        field_role: field_role.to_string(),
        normalized_view: normalized_view.map(str::to_string),
        required,
    }
}

fn package_ref(kind: ProfilePackageRefKind, suffix: &str, digest_char: char) -> ProfilePackageRef {
    ProfilePackageRef {
        kind,
        id: format!("{PROFILE_ID}.{suffix}"),
        version: "2026.09.03".to_string(),
        content_hash: format!("blake3:{}", digest_char.to_string().repeat(64)),
    }
}

fn surface_cores(work_dir: &Path) -> BTreeMap<String, String> {
    read_jsonl::<PreparedSurfaceRecord>(&work_dir.join("prepare/surfaces.jsonl"))
        .into_iter()
        .map(|surface| {
            let core = surface
                .normalized_views
                .get("core")
                .expect("core view")
                .value
                .clone();
            (surface.surface_id, core)
        })
        .collect()
}

fn record_for_cores<'a>(
    records: &'a [EdgeEvidenceRecord],
    cores: &BTreeMap<String, String>,
    left: &str,
    right: &str,
) -> &'a EdgeEvidenceRecord {
    records
        .iter()
        .find(|record| {
            let Some(left_core) = cores.get(&record.left_surface_id) else {
                return false;
            };
            let Some(right_core) = cores.get(&record.right_surface_id) else {
                return false;
            };
            (left_core == left && right_core == right) || (left_core == right && right_core == left)
        })
        .unwrap_or_else(|| panic!("expected evidence pair {left:?} <> {right:?}"))
}

fn record_has_core(
    record: &EdgeEvidenceRecord,
    cores: &BTreeMap<String, String>,
    expected: &str,
) -> bool {
    cores
        .get(&record.left_surface_id)
        .is_some_and(|core| core == expected)
        || cores
            .get(&record.right_surface_id)
            .is_some_and(|core| core == expected)
}

fn support_hit<'a>(
    record: &'a EdgeEvidenceRecord,
    operator_id: &str,
) -> Option<&'a EdgeEvidenceHit> {
    record
        .hits
        .iter()
        .find(|hit| hit.lane == ScoreLane::Support && hit.operator_id == operator_id)
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl record parses"))
        .collect()
}
