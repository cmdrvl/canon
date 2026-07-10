#![forbid(unsafe_code)]

#[path = "../src/evaluation/corpus.rs"]
mod corpus;

use corpus::{
    AdjudicationDecision, AdjudicationRecord, AssignmentLabel, ClusterLabel, CorpusDataset,
    CorpusExecutionPolicy, CorpusLicenseGrant, CorpusObservation, CorpusPartition,
    CorpusProvenance, CorpusRedactionClass, CrossDatasetPair, DatasetStorageKind, EvaluationCorpus,
    EvaluationErrorCode, HardNegativeLabel, HardNegativeSeverity, IdentifierEvaluationLabel,
    ObservationLocator, ObservationLocatorKind, PairDisposition, RelationshipDisposition,
    RelationshipLabel, TemporalChangeLabel, TemporalDisposition, canonical_corpus_bytes,
    deterministic_metrics, evaluation_corpus_schema_version, finalize_corpus,
};
use serde_json::{Value, json};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.evaluation.corpus.v1.schema.json");

#[test]
fn schema_declares_private_public_leakage_and_exact_replay_contracts() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], evaluation_corpus_schema_version());
    assert_eq!(
        schema["properties"]["version"]["const"],
        evaluation_corpus_schema_version()
    );
    assert_eq!(
        schema["x-canon-contract"]["manifest_shape_shared_across_private_and_public_corpora"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["holdout_and_exact_replay_sealed_from_tuning"],
        true
    );
    assert_eq!(
        schema["$defs"]["partition"]["enum"],
        json!(["train", "tune", "holdout", "exact_replay"])
    );
    assert!(
        schema["$defs"]["observation"]["properties"]
            .get("raw_content")
            .is_none()
    );
}

#[test]
fn private_and_public_corpora_share_manifest_without_inline_content() {
    let corpus = finalize_corpus(sample_corpus()).expect("corpus finalizes");
    assert!(
        corpus
            .datasets
            .iter()
            .any(|dataset| { dataset.storage_kind == DatasetStorageKind::PublicFixture })
    );
    assert!(
        corpus
            .datasets
            .iter()
            .any(|dataset| { dataset.storage_kind == DatasetStorageKind::PrivatePathRef })
    );

    let serialized = serde_json::to_value(&corpus).expect("corpus serializes");
    assert!(
        serialized["observations"][0]["locator"]["content_digest"]
            .as_str()
            .is_some()
    );
    assert!(
        serialized["observations"][0]
            .as_object()
            .expect("observation object")
            .get("raw_content")
            .is_none()
    );
}

#[test]
fn execution_policy_keeps_holdout_and_exact_replay_out_of_tuning() {
    let mut corpus = sample_corpus();
    corpus
        .execution_policy
        .tuning_partitions
        .push(CorpusPartition::ExactReplay);

    let error = finalize_corpus(corpus).expect_err("exact replay must stay out of tuning");
    assert_eq!(error.code, EvaluationErrorCode::CompatibilityPolicy);
}

#[test]
fn shared_split_groups_across_partitions_are_detected_as_leakage() {
    let mut corpus = sample_corpus();
    let replay_observation = corpus
        .observations
        .iter_mut()
        .find(|observation| observation.observation_id == "obs.exact.replay")
        .expect("replay observation");
    replay_observation.split_group_id = "grp.hold.alpha".to_string();

    let error = finalize_corpus(corpus).expect_err("leakage must fail");
    assert_eq!(error.code, EvaluationErrorCode::PartitionLeakage);
}

#[test]
fn public_license_and_private_redaction_gaps_are_detected() {
    let mut public_license_gap = sample_corpus();
    let train_dataset = public_license_gap
        .datasets
        .iter_mut()
        .find(|dataset| dataset.dataset_id == "dataset.train.public")
        .expect("train dataset");
    train_dataset.license_id = "license.private.internal".to_string();
    let error = finalize_corpus(public_license_gap)
        .expect_err("public dataset requires redistributable license");
    assert_eq!(error.code, EvaluationErrorCode::LicenseGap);

    let mut private_redaction_gap = sample_corpus();
    let holdout_dataset = private_redaction_gap
        .datasets
        .iter_mut()
        .find(|dataset| dataset.dataset_id == "dataset.holdout.private")
        .expect("holdout dataset");
    holdout_dataset.redaction_id = "redaction.public_fixture".to_string();
    let error = finalize_corpus(private_redaction_gap)
        .expect_err("private path requires private redaction");
    assert_eq!(error.code, EvaluationErrorCode::RedactionGap);
}

#[test]
fn hard_negatives_cannot_overlap_positive_cluster_labels() {
    let mut corpus = sample_corpus();
    corpus.hard_negatives.push(HardNegativeLabel {
        hard_negative_id: "hn.conflict".to_string(),
        left_observation_id: "obs.hold.alpha".to_string(),
        right_observation_id: "obs.hold.beta".to_string(),
        severity: HardNegativeSeverity::High,
        reason_code: "anti_merge_conflict".to_string(),
        adjudication_ref: "adj.high".to_string(),
    });

    let error = finalize_corpus(corpus).expect_err("positive cluster overlap must fail");
    assert_eq!(error.code, EvaluationErrorCode::InconsistentLabel);
}

#[test]
fn metrics_support_unknown_ontology_role_ids_and_abstention() {
    let metrics = deterministic_metrics(&sample_corpus()).expect("metrics compute");
    assert_eq!(metrics.observation_count, 5);
    assert_eq!(metrics.cluster_label_count, 2);
    assert_eq!(metrics.dataset_counts_by_partition["exact_replay"], 1);
    assert_eq!(metrics.pair_counts_by_disposition["abstain"], 1);
    assert_eq!(metrics.pair_counts_by_disposition["distinct_entity"], 1);
    assert_eq!(metrics.hard_negative_counts_by_severity["critical"], 1);
    assert_eq!(
        metrics.relationship_counts_by_type["ontology.synthetic:ownership_arc::role.synthetic:parent"],
        1
    );
    assert_eq!(
        metrics.assignment_counts_by_type["ontology.synthetic:workflow::role.synthetic:approver"],
        1
    );
    assert_eq!(
        metrics.temporal_change_counts_by_type["ontology.synthetic:history::change.synthetic:renamed"],
        1
    );
    assert_eq!(metrics.adjudication_confidence_bands["high"], 1);
    assert_eq!(metrics.exact_replay_coverage.observation_count, 1);
    assert_eq!(metrics.exact_replay_coverage.pair_label_count, 1);
    assert_eq!(metrics.exact_replay_coverage.hard_negative_count, 1);
}

#[test]
fn canonical_bytes_and_metrics_are_stable_across_reordered_inputs() {
    let left = sample_corpus();
    let mut right = sample_corpus();
    right.datasets.reverse();
    right.observations.reverse();
    right.adjudications.reverse();
    right.cluster_labels.reverse();
    right.cluster_labels[0].observation_ids.reverse();
    right.cross_dataset_pairs.reverse();
    right.identifiers.reverse();
    right.hard_negatives.reverse();
    right.relationships.reverse();
    right.assignments.reverse();
    right.temporal_changes.reverse();

    let left_bytes = canonical_corpus_bytes(&left).expect("left bytes");
    let right_bytes = canonical_corpus_bytes(&right).expect("right bytes");
    assert_eq!(left_bytes, right_bytes);

    let left_metrics = serde_json::to_value(deterministic_metrics(&left).expect("left metrics"))
        .expect("left metrics serialize");
    let right_metrics = serde_json::to_value(deterministic_metrics(&right).expect("right metrics"))
        .expect("right metrics serialize");
    assert_eq!(left_metrics, right_metrics);
}

fn sample_corpus() -> EvaluationCorpus {
    EvaluationCorpus {
        version: String::new(),
        corpus_id: "pkg.synthetic.evaluation".to_string(),
        corpus_version: "1.2.3".to_string(),
        provenance: CorpusProvenance {
            manifest_locator: "manifests/evaluation_corpus.json".to_string(),
            source_snapshot_locator: "s3://synthetic/eval/snapshot-2026-07".to_string(),
            source_snapshot_digest: sample_hash('a'),
            generated_at: "2026-07-10T20:00:00Z".to_string(),
            exact_replay_runner_ref: "runner.synthetic:exact_replay_v1".to_string(),
        },
        licenses: vec![
            CorpusLicenseGrant {
                license_id: "license.public.cc0".to_string(),
                license_expression: "CC0-1.0".to_string(),
                redistributable: true,
                attribution_required: false,
                usage_notice: "Public synthetic fixtures only.".to_string(),
            },
            CorpusLicenseGrant {
                license_id: "license.private.internal".to_string(),
                license_expression: "LicenseRef-Internal".to_string(),
                redistributable: false,
                attribution_required: true,
                usage_notice: "Private operator-owned labels stay out of tree.".to_string(),
            },
        ],
        redaction_classes: vec![
            CorpusRedactionClass {
                redaction_id: "redaction.public_fixture".to_string(),
                raw_content_retained: false,
                export_surface_fingerprints_only: true,
                private_path_allowed: false,
            },
            CorpusRedactionClass {
                redaction_id: "redaction.private_locator".to_string(),
                raw_content_retained: false,
                export_surface_fingerprints_only: true,
                private_path_allowed: true,
            },
        ],
        execution_policy: CorpusExecutionPolicy {
            tuning_partitions: vec![CorpusPartition::Train, CorpusPartition::Tune],
            scoring_partitions: vec![CorpusPartition::Holdout, CorpusPartition::ExactReplay],
            holdout_labels_sealed_from_tuning: true,
            exact_replay_partition_separate: true,
        },
        datasets: vec![
            CorpusDataset {
                dataset_id: "dataset.train.public".to_string(),
                partition: CorpusPartition::Train,
                storage_kind: DatasetStorageKind::PublicFixture,
                source_locator: "tests/fixtures/eval/train.jsonl".to_string(),
                content_digest: sample_hash('b'),
                license_id: "license.public.cc0".to_string(),
                redaction_id: "redaction.public_fixture".to_string(),
            },
            CorpusDataset {
                dataset_id: "dataset.tune.public".to_string(),
                partition: CorpusPartition::Tune,
                storage_kind: DatasetStorageKind::PublicFixture,
                source_locator: "tests/fixtures/eval/tune.jsonl".to_string(),
                content_digest: sample_hash('c'),
                license_id: "license.public.cc0".to_string(),
                redaction_id: "redaction.public_fixture".to_string(),
            },
            CorpusDataset {
                dataset_id: "dataset.holdout.private".to_string(),
                partition: CorpusPartition::Holdout,
                storage_kind: DatasetStorageKind::PrivatePathRef,
                source_locator: "/secure/evaluation/holdout.jsonl".to_string(),
                content_digest: sample_hash('d'),
                license_id: "license.private.internal".to_string(),
                redaction_id: "redaction.private_locator".to_string(),
            },
            CorpusDataset {
                dataset_id: "dataset.exact.private".to_string(),
                partition: CorpusPartition::ExactReplay,
                storage_kind: DatasetStorageKind::PrivatePathRef,
                source_locator: "/secure/evaluation/exact_replay.jsonl".to_string(),
                content_digest: sample_hash('e'),
                license_id: "license.private.internal".to_string(),
                redaction_id: "redaction.private_locator".to_string(),
            },
        ],
        observations: vec![
            observation(
                "obs.train.alpha",
                "dataset.train.public",
                "subject.synthetic.alpha",
                "grp.train.alpha",
                'f',
                'g',
                "row:1",
            ),
            observation(
                "obs.tune.beta",
                "dataset.tune.public",
                "subject.synthetic.beta",
                "grp.tune.beta",
                'h',
                'i',
                "row:2",
            ),
            observation(
                "obs.hold.alpha",
                "dataset.holdout.private",
                "subject.synthetic.hold_alpha",
                "grp.hold.alpha",
                'j',
                'k',
                "row:3",
            ),
            observation(
                "obs.hold.beta",
                "dataset.holdout.private",
                "subject.synthetic.hold_beta",
                "grp.hold.beta",
                'l',
                'm',
                "row:4",
            ),
            observation(
                "obs.exact.replay",
                "dataset.exact.private",
                "subject.synthetic.replay",
                "grp.exact.replay",
                'n',
                'o',
                "row:5",
            ),
        ],
        adjudications: vec![
            adjudication("adj.high", AdjudicationDecision::Accepted, 9800, 'p', 'q'),
            adjudication("adj.medium", AdjudicationDecision::Accepted, 7200, 'r', 's'),
            adjudication("adj.abstain", AdjudicationDecision::Abstain, 6100, 't', 'u'),
            adjudication("adj.low", AdjudicationDecision::Rejected, 4300, 'v', 'w'),
        ],
        cluster_labels: vec![
            ClusterLabel {
                cluster_id: "cluster.hold".to_string(),
                observation_ids: vec!["obs.hold.alpha".to_string(), "obs.hold.beta".to_string()],
                adjudication_ref: "adj.high".to_string(),
            },
            ClusterLabel {
                cluster_id: "cluster.train.singleton".to_string(),
                observation_ids: vec!["obs.train.alpha".to_string()],
                adjudication_ref: "adj.medium".to_string(),
            },
        ],
        cross_dataset_pairs: vec![
            CrossDatasetPair {
                pair_id: "pair.distinct".to_string(),
                left_observation_id: "obs.tune.beta".to_string(),
                right_observation_id: "obs.hold.alpha".to_string(),
                disposition: PairDisposition::DistinctEntity,
                adjudication_ref: "adj.medium".to_string(),
            },
            CrossDatasetPair {
                pair_id: "pair.abstain".to_string(),
                left_observation_id: "obs.train.alpha".to_string(),
                right_observation_id: "obs.exact.replay".to_string(),
                disposition: PairDisposition::Abstain,
                adjudication_ref: "adj.abstain".to_string(),
            },
        ],
        identifiers: vec![
            IdentifierEvaluationLabel {
                identifier_id: "identifier.hold.alpha".to_string(),
                observation_id: "obs.hold.alpha".to_string(),
                namespace_id: "namespace.synthetic:ledger".to_string(),
                value_fingerprint: sample_hash('x'),
                adjudication_ref: "adj.high".to_string(),
            },
            IdentifierEvaluationLabel {
                identifier_id: "identifier.replay".to_string(),
                observation_id: "obs.exact.replay".to_string(),
                namespace_id: "namespace.synthetic:ledger".to_string(),
                value_fingerprint: sample_hash('y'),
                adjudication_ref: "adj.medium".to_string(),
            },
        ],
        hard_negatives: vec![HardNegativeLabel {
            hard_negative_id: "hn.critical".to_string(),
            left_observation_id: "obs.tune.beta".to_string(),
            right_observation_id: "obs.exact.replay".to_string(),
            severity: HardNegativeSeverity::Critical,
            reason_code: "false_merge_family".to_string(),
            adjudication_ref: "adj.high".to_string(),
        }],
        relationships: vec![RelationshipLabel {
            relationship_id: "rel.synthetic.arc".to_string(),
            left_observation_id: "obs.hold.alpha".to_string(),
            right_observation_id: "obs.hold.beta".to_string(),
            ontology_id: "ontology.synthetic:ownership_arc".to_string(),
            role_id: "role.synthetic:parent".to_string(),
            disposition: RelationshipDisposition::Present,
            adjudication_ref: "adj.high".to_string(),
        }],
        assignments: vec![AssignmentLabel {
            assignment_id: "assignment.synthetic.workflow".to_string(),
            observation_id: "obs.hold.alpha".to_string(),
            assignee_key: "queue.synthetic:amber".to_string(),
            ontology_id: "ontology.synthetic:workflow".to_string(),
            role_id: "role.synthetic:approver".to_string(),
            adjudication_ref: "adj.medium".to_string(),
        }],
        temporal_changes: vec![TemporalChangeLabel {
            change_id: "change.synthetic.history".to_string(),
            previous_observation_id: "obs.hold.alpha".to_string(),
            next_observation_id: "obs.hold.beta".to_string(),
            ontology_id: "ontology.synthetic:history".to_string(),
            change_kind_id: "change.synthetic:renamed".to_string(),
            disposition: TemporalDisposition::Changed,
            adjudication_ref: "adj.low".to_string(),
        }],
    }
}

fn observation(
    observation_id: &str,
    dataset_id: &str,
    subject_key: &str,
    split_group_id: &str,
    surface_hash: char,
    content_hash: char,
    locator: &str,
) -> CorpusObservation {
    CorpusObservation {
        observation_id: observation_id.to_string(),
        dataset_id: dataset_id.to_string(),
        subject_key: subject_key.to_string(),
        split_group_id: split_group_id.to_string(),
        surface_fingerprint: sample_hash(surface_hash),
        locator: ObservationLocator {
            kind: ObservationLocatorKind::CsvRow,
            locator: locator.to_string(),
            content_digest: sample_hash(content_hash),
        },
        observed_at: Some("2026-07-10T00:00:00Z".to_string()),
    }
}

fn adjudication(
    adjudication_id: &str,
    decision: AdjudicationDecision,
    confidence_basis_points: u16,
    reviewer_hash: char,
    note_hash: char,
) -> AdjudicationRecord {
    AdjudicationRecord {
        adjudication_id: adjudication_id.to_string(),
        decision,
        confidence_basis_points,
        reviewer_set_digest: sample_hash(reviewer_hash),
        note_digest: sample_hash(note_hash),
    }
}

fn sample_hash(character: char) -> String {
    format!(
        "blake3:{}",
        blake3::hash(character.to_string().as_bytes()).to_hex()
    )
}
