#![forbid(unsafe_code)]

pub mod registry {
    pub use canon::registry::*;
}

pub use canon::RegistryDiffEntry;

mod temporal_impl {
    pub mod fact {
        #![allow(dead_code)]
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/temporal/fact.rs"));
    }

    pub use fact::*;

    pub fn finalize_fact(fact: fact::IdentityFact) -> fact::TemporalResult<fact::IdentityFact> {
        let canon_fact = serde_json::from_value(
            serde_json::to_value(fact).expect("local fact serializes to canon fact"),
        )
        .expect("local fact shape matches canon fact");
        let finalized = canon::temporal::finalize_fact(canon_fact).map_err(convert_error)?;
        serde_json::from_value(
            serde_json::to_value(finalized).expect("canon fact serializes to local fact"),
        )
        .map_err(|error| {
            fact::TemporalError::new(
                fact::TemporalErrorCode::ArtifactContract,
                format!("failed to convert finalized fact: {error}"),
            )
        })
    }

    pub fn finalize_facts(
        facts: impl IntoIterator<Item = fact::IdentityFact>,
    ) -> fact::TemporalResult<Vec<fact::IdentityFact>> {
        let canon_facts = facts
            .into_iter()
            .map(|fact| {
                serde_json::from_value(
                    serde_json::to_value(fact).expect("local fact serializes to canon fact"),
                )
                .expect("local fact shape matches canon fact")
            })
            .collect::<Vec<_>>();
        let finalized = canon::temporal::finalize_facts(canon_facts).map_err(convert_error)?;
        finalized
            .into_iter()
            .map(|fact| {
                serde_json::from_value(
                    serde_json::to_value(fact).expect("canon fact serializes to local fact"),
                )
                .map_err(|error| {
                    fact::TemporalError::new(
                        fact::TemporalErrorCode::ArtifactContract,
                        format!("failed to convert finalized fact: {error}"),
                    )
                })
            })
            .collect()
    }

    fn convert_error(error: canon::temporal::TemporalError) -> fact::TemporalError {
        serde_json::from_value(
            serde_json::to_value(error).expect("canon temporal error serializes"),
        )
        .expect("canon temporal error shape matches local temporal error")
    }

    pub mod alias {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/alias.rs"
        ));
    }

    pub mod conflict {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/conflict.rs"
        ));
    }

    pub mod compile {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/compile.rs"
        ));
    }

    pub mod explain {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/explain.rs"
        ));
    }

    pub mod diff {
        #![allow(dead_code)]
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/temporal/diff.rs"));
    }
}

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporal_impl::diff::{
    CANON_TEMPORAL_DIFF_VERSION, TemporalDiffFilter, TemporalDiffPageRequest, TemporalDiffRequest,
    diff_temporal_snapshots,
};
use temporal_impl::explain::{
    TemporalChangeClass, TemporalExactResult, TemporalIdentitySnapshot, TemporalSnapshotReference,
};
use temporal_impl::fact::{
    AssertionStatus, FactScope, IdentityFact, IntervalBoundary, RecordedTime, SourceLocator,
    TimeInterval,
};
use temporal_impl::finalize_fact;

const FIXTURE_DIR: &str = "tests/fixtures/temporal/neutral-history";

#[test]
fn neutral_history_subprocess_journey_matches_expected_snapshot() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("CANON_TEMPORAL_E2E_WORKER").is_some() {
        return Ok(());
    }

    let temp = tempfile::tempdir()?;
    let output_path = temp.path().join("temporal-neutral-output.json");
    let worker = Command::new(std::env::current_exe()?)
        .arg("--ignored")
        .arg("--exact")
        .arg("temporal_e2e_worker_entrypoint")
        .env("CANON_TEMPORAL_E2E_WORKER", "1")
        .env("CANON_TEMPORAL_E2E_FIXTURE_DIR", FIXTURE_DIR)
        .env("CANON_TEMPORAL_E2E_OUTPUT", &output_path)
        .output()?;

    assert!(
        worker.status.success(),
        "worker failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&worker.stdout),
        String::from_utf8_lossy(&worker.stderr)
    );

    let actual: Value = serde_json::from_str(&fs::read_to_string(&output_path)?)?;
    let expected: Value = serde_json::from_str(&fs::read_to_string(
        Path::new(FIXTURE_DIR).join("expected_snapshot.json"),
    )?)?;

    assert_eq!(actual["scenario"], expected["scenario"]);
    assert_eq!(actual["before_mappings"], expected["before_mappings"]);
    assert_eq!(actual["after_mappings"], expected["after_mappings"]);
    assert_eq!(actual["change_classes"], expected["change_classes"]);
    assert_eq!(actual["diff_summary"], expected["diff_summary"]);
    assert_eq!(actual["evidence"]["rename"], expected["evidence"]["rename"]);
    assert_eq!(actual["evidence"]["merge"], expected["evidence"]["merge"]);
    assert_eq!(actual["evidence"]["split"], expected["evidence"]["split"]);
    assert_eq!(
        actual["evidence"]["succession"],
        expected["evidence"]["succession"]
    );
    assert_eq!(
        actual["evidence"]["late_correction"],
        expected["evidence"]["late_correction"]
    );
    assert!(
        actual["artifact_hash"]
            .as_str()
            .expect("artifact hash string")
            .starts_with("blake3:")
    );
    Ok(())
}

#[test]
#[ignore = "subprocess worker invoked by neutral_history_subprocess_journey_matches_expected_snapshot"]
fn temporal_e2e_worker_entrypoint() {
    if std::env::var_os("CANON_TEMPORAL_E2E_WORKER").is_none() {
        return;
    }
    let fixture_dir = PathBuf::from(
        std::env::var("CANON_TEMPORAL_E2E_FIXTURE_DIR").expect("fixture dir env var"),
    );
    let output_path =
        PathBuf::from(std::env::var("CANON_TEMPORAL_E2E_OUTPUT").expect("output env var"));
    let report = build_fixture_report(&fixture_dir).expect("fixture report builds");
    let bytes = serde_json::to_vec_pretty(&report).expect("report serializes");
    fs::write(output_path, bytes).expect("report writes");
}

fn build_fixture_report(fixture_dir: &Path) -> Result<Value, Box<dyn Error>> {
    let strategy: Strategy =
        serde_yaml::from_str(&fs::read_to_string(fixture_dir.join("strategy.yaml"))?)?;
    let mut facts_by_row = BTreeMap::new();
    let mut all_facts = Vec::new();

    for row in read_rows(&fixture_dir.join("rows.csv"))? {
        let fact = finalize_fact(row.to_fact(Vec::new(), Vec::new())?)?;
        facts_by_row.insert(row.row_id.clone(), fact.clone());
        all_facts.push(fact);
    }

    for correction in read_corrections(&fixture_dir.join("corrections.jsonl"))? {
        let supersedes = linked_fact_ids(&facts_by_row, &correction.supersedes)?;
        let retracts = linked_fact_ids(&facts_by_row, &correction.retracts)?;
        let fact = finalize_fact(correction.row.to_fact(supersedes, retracts)?)?;
        facts_by_row.insert(correction.row.row_id.clone(), fact.clone());
        all_facts.push(fact);
    }

    let before_spec = strategy.snapshot("before")?;
    let after_spec = strategy.snapshot("after")?;
    let before = snapshot(&strategy, before_spec, all_facts.clone());
    let after = snapshot(&strategy, after_spec, all_facts);
    let diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: before.clone(),
        after: after.clone(),
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest {
            limit: 100,
            after_cursor: None,
        },
        include_unchanged: true,
    })?;

    let before_mappings = active_mappings(&before)?;
    let after_mappings = active_mappings(&after)?;
    let change_classes = diff
        .changes
        .iter()
        .map(|change| {
            (
                change.subject_id.clone(),
                serde_json::to_value(change.change_class).expect("change class serializes"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let changed = diff
        .changes
        .iter()
        .filter(|change| change.change_class != TemporalChangeClass::NoChange)
        .count();

    let report_without_hash = json!({
        "scenario": strategy.scenario,
        "before_mappings": before_mappings,
        "after_mappings": after_mappings,
        "change_classes": change_classes,
        "diff_summary": {
            "compared_subject_count": diff.summary.compared_subject_count,
            "changed_subject_count": changed,
            "total_matching_change_count": diff.summary.total_matching_change_count,
            "by_change_class": diff.summary.by_change_class,
        },
        "evidence": classify_evidence(&diff.changes),
    });
    let artifact_hash = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&report_without_hash)?).to_hex()
    );
    let mut report = report_without_hash;
    report["artifact_hash"] = Value::String(artifact_hash);
    Ok(report)
}

fn read_rows(path: &Path) -> Result<Vec<FactRow>, Box<dyn Error>> {
    let mut reader = csv::Reader::from_path(path)?;
    reader
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn read_corrections(path: &Path) -> Result<Vec<CorrectionRow>, Box<dyn Error>> {
    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

fn linked_fact_ids(
    facts_by_row: &BTreeMap<String, IdentityFact>,
    row_ids: &[String],
) -> Result<Vec<String>, Box<dyn Error>> {
    row_ids
        .iter()
        .map(|row_id| {
            facts_by_row
                .get(row_id)
                .map(|fact| fact.fact_id.clone())
                .ok_or_else(|| format!("unknown linked row_id {row_id}").into())
        })
        .collect()
}

fn snapshot(
    strategy: &Strategy,
    spec: &SnapshotSpec,
    facts: Vec<IdentityFact>,
) -> TemporalIdentitySnapshot {
    TemporalIdentitySnapshot {
        snapshot: TemporalSnapshotReference {
            snapshot_id: spec.id.clone(),
            registry_id: strategy.registry_id.clone(),
            registry_version: strategy.registry_version.clone(),
            compiled_snapshot_digest: digest_for(&format!(
                "{}:{}:{}",
                strategy.registry_id, spec.valid_at, spec.known_as_of
            )),
            valid_at: spec.valid_at.clone(),
            known_as_of: spec.known_as_of.clone(),
            policy_ref: strategy.policy_ref.clone(),
            policy_version: spec.policy_version.clone(),
        },
        facts,
        relationships: Vec::new(),
    }
}

fn active_mappings(
    snapshot: &TemporalIdentitySnapshot,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut mappings = BTreeMap::new();
    for subject_id in active_subject_ids(snapshot) {
        let subject = temporal_impl::explain::TemporalExplainSubject::Surface {
            subject_id: subject_id.clone(),
        };
        let result = temporal_impl::explain::explain_snapshot_result(snapshot, &subject, None)?;
        if let TemporalExactResult::SurfaceMapping { canonical_id, .. } = result.exact_result {
            mappings.insert(subject_id, canonical_id);
        }
    }
    Ok(mappings)
}

fn active_subject_ids(snapshot: &TemporalIdentitySnapshot) -> BTreeSet<String> {
    snapshot
        .facts
        .iter()
        .map(|fact| fact.subject_id.clone())
        .collect()
}

fn classify_evidence(
    changes: &[temporal_impl::diff::TemporalDiffChange],
) -> BTreeMap<String, Vec<String>> {
    let mut evidence = BTreeMap::from([
        ("rename".to_string(), Vec::new()),
        ("merge".to_string(), Vec::new()),
        ("split".to_string(), Vec::new()),
        ("succession".to_string(), Vec::new()),
        ("late_correction".to_string(), Vec::new()),
    ]);
    for change in changes {
        match change.subject_id.as_str() {
            "alias:orion-renamed" => evidence
                .get_mut("rename")
                .expect("rename key")
                .push(change.subject_id.clone()),
            "alias:mesa-left" | "alias:mesa-right" => evidence
                .get_mut("merge")
                .expect("merge key")
                .push(change.subject_id.clone()),
            "alias:river-platform" | "alias:river-east" | "alias:river-west" => evidence
                .get_mut("split")
                .expect("split key")
                .push(change.subject_id.clone()),
            "alias:harbor-license" => evidence
                .get_mut("succession")
                .expect("succession key")
                .push(change.subject_id.clone()),
            "alias:ledger" => evidence
                .get_mut("late_correction")
                .expect("late correction key")
                .push(change.subject_id.clone()),
            _ => {}
        }
    }
    for subjects in evidence.values_mut() {
        subjects.sort();
        subjects.dedup();
    }
    evidence
}

#[derive(Debug, Deserialize)]
struct Strategy {
    scenario: String,
    registry_id: String,
    registry_version: String,
    policy_ref: String,
    snapshots: Vec<SnapshotSpec>,
}

impl Strategy {
    fn snapshot(&self, id: &str) -> Result<&SnapshotSpec, Box<dyn Error>> {
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.id == id)
            .ok_or_else(|| format!("missing snapshot {id}").into())
    }
}

#[derive(Debug, Deserialize)]
struct SnapshotSpec {
    id: String,
    valid_at: String,
    known_as_of: String,
    policy_version: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CorrectionRow {
    #[serde(flatten)]
    row: FactRow,
    #[serde(default)]
    supersedes: Vec<String>,
    #[serde(default)]
    retracts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FactRow {
    row_id: String,
    subject_id: String,
    canonical_id: String,
    source_system: String,
    valid_from: String,
    valid_to: String,
    recorded_at: String,
    transaction_seq: u64,
    assertion_status: AssertionStatus,
    scope_type: String,
    scope_id: String,
    digest_seed: String,
}

impl FactRow {
    fn to_fact(
        &self,
        supersedes: Vec<String>,
        retracts: Vec<String>,
    ) -> Result<IdentityFact, Box<dyn Error>> {
        let digest_seed = self
            .digest_seed
            .chars()
            .next()
            .ok_or("digest_seed is required")?;
        Ok(IdentityFact {
            version: String::new(),
            fact_id: String::new(),
            assertion_key: String::new(),
            conflict_key: String::new(),
            subject_id: self.subject_id.clone(),
            predicate: "same_as".to_string(),
            object_id: self.canonical_id.clone(),
            valid_time: TimeInterval {
                start_at: Some(self.valid_from.clone()),
                start_bound: IntervalBoundary::Inclusive,
                end_at: Some(self.valid_to.clone()),
                end_bound: IntervalBoundary::Inclusive,
            },
            recorded_time: RecordedTime {
                start_at: Some(self.recorded_at.clone()),
                start_bound: IntervalBoundary::Inclusive,
                end_at: None,
                end_bound: IntervalBoundary::Open,
                transaction_seq: Some(self.transaction_seq),
            },
            source_locator: SourceLocator {
                source_system: self.source_system.clone(),
                locator: format!("neutral-history/{}", self.row_id),
                fragment: Some(self.row_id.clone()),
            },
            materialization_digest: sample_hash(digest_seed),
            assertion_status: self.assertion_status,
            trust_policy_ref: "trust.neutral-history.v1".to_string(),
            scope: Some(FactScope {
                scope_type: self.scope_type.clone(),
                scope_id: self.scope_id.clone(),
            }),
            supersedes,
            retracts,
        })
    }
}

fn sample_hash(seed: char) -> String {
    let hex = if seed.is_ascii_hexdigit() { seed } else { 'a' };
    format!("blake3:{}", hex.to_ascii_lowercase().to_string().repeat(64))
}

fn digest_for(value: &str) -> String {
    format!("blake3:{}", blake3::hash(value.as_bytes()).to_hex())
}
