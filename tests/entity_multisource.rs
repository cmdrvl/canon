#![forbid(unsafe_code)]

use canon::RefusalCode;
use canon::entity::run::link::{
    LINK_SIDE_COLUMN, LINK_SOURCE_NAME_COLUMN, LINK_SOURCE_ORDINAL_COLUMN, LINK_SOURCE_ROW_COLUMN,
    multisource::{
        ENTITY_MULTISOURCE_LINK_VERSION, EntityMultisourceLinkRequest, EntityNamedSource,
        EntitySourceComparison, EntitySourceRole, complete_comparison_graph,
        materialize_multisource_rows,
    },
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn multisource_materialization_is_invariant_to_source_and_edge_order() {
    let fixture = MultiSourceFixture::new();
    let output_a = fixture.work_dir.join("a.csv");
    let output_b = fixture.work_dir.join("b.csv");

    let artifact_a = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source(
                "alpha",
                EntitySourceRole::CanonicalReference,
                &fixture.alpha,
            ),
            source("beta", EntitySourceRole::Target, &fixture.beta),
            source("gamma", EntitySourceRole::Peer, &fixture.gamma),
        ],
        comparison_graph: vec![
            EntitySourceComparison::new("alpha", "beta"),
            EntitySourceComparison::new("beta", "gamma"),
            EntitySourceComparison::new("alpha", "gamma"),
        ],
        canonical_source: Some("alpha"),
        default_pair_budget: 16,
        output_rows: &output_a,
    })
    .expect("first multisource materialization");

    let artifact_b = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("gamma", EntitySourceRole::Peer, &fixture.gamma),
            source(
                "alpha",
                EntitySourceRole::CanonicalReference,
                &fixture.alpha,
            ),
            source("beta", EntitySourceRole::Target, &fixture.beta),
        ],
        comparison_graph: vec![
            EntitySourceComparison::new("gamma", "beta"),
            EntitySourceComparison::new("alpha", "gamma"),
            EntitySourceComparison::new("beta", "alpha"),
        ],
        canonical_source: Some("alpha"),
        default_pair_budget: 16,
        output_rows: &output_b,
    })
    .expect("permuted multisource materialization");

    assert_eq!(artifact_a.version, ENTITY_MULTISOURCE_LINK_VERSION);
    assert_eq!(artifact_a.source_count, 3);
    assert_eq!(artifact_a.row_count, 6);
    assert_eq!(artifact_a.canonical_source.as_deref(), Some("alpha"));
    assert_eq!(artifact_a.sources, artifact_b.sources);
    assert_eq!(artifact_a.comparison_graph, artifact_b.comparison_graph);
    assert_eq!(artifact_a.consistency, artifact_b.consistency);
    assert_eq!(
        strip_path(artifact_a.clone()),
        strip_path(artifact_b),
        "artifact semantics are independent of input enumeration"
    );

    let rows_a = fs::read_to_string(output_a).expect("materialized rows a");
    let rows_b = fs::read_to_string(output_b).expect("materialized rows b");
    assert_eq!(rows_a, rows_b);
    let header = rows_a.lines().next().expect("header");
    for reserved in [
        LINK_SOURCE_NAME_COLUMN,
        LINK_SIDE_COLUMN,
        LINK_SOURCE_ROW_COLUMN,
        LINK_SOURCE_ORDINAL_COLUMN,
    ] {
        assert!(
            header.contains(reserved),
            "missing reserved header {reserved}"
        );
    }
    assert_eq!(
        source_names_in_rows(&rows_a),
        ["alpha", "beta", "gamma"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

#[test]
fn sparse_graph_pair_budgets_refuse_hot_pairs_deterministically() {
    let fixture = MultiSourceFixture::new();
    let refusal = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("alpha", EntitySourceRole::Reference, &fixture.alpha),
            source("beta", EntitySourceRole::Target, &fixture.beta),
            source("gamma", EntitySourceRole::Peer, &fixture.gamma),
        ],
        comparison_graph: vec![
            EntitySourceComparison::with_budget("alpha", "beta", 3),
            EntitySourceComparison::with_budget("beta", "gamma", 8),
        ],
        canonical_source: None,
        default_pair_budget: 99,
        output_rows: &fixture.work_dir.join("budget.csv"),
    })
    .expect_err("alpha/beta pair exceeds explicit budget");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["reason"], "pair_budget_exceeded");
    assert_eq!(refusal.detail["left_source"], "alpha");
    assert_eq!(refusal.detail["right_source"], "beta");
    assert_eq!(refusal.detail["candidate_pair_rows"], 4);
    assert_eq!(refusal.detail["max_candidate_rows"], 3);
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn anchor_conflicts_surface_abstentions_instead_of_transitive_forcing() {
    let fixture = MultiSourceFixture::new();
    let artifact = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            anchored_source("alpha", EntitySourceRole::Reference, &fixture.alpha),
            anchored_source("beta", EntitySourceRole::Target, &fixture.beta),
            anchored_source("gamma", EntitySourceRole::Peer, &fixture.gamma),
        ],
        comparison_graph: complete_comparison_graph(["alpha", "beta", "gamma"]),
        canonical_source: Some("alpha"),
        default_pair_budget: 16,
        output_rows: &fixture.work_dir.join("conflict.csv"),
    })
    .expect("conflict artifact materializes");

    assert_eq!(artifact.consistency.anchor_conflicts.len(), 1);
    let conflict = &artifact.consistency.anchor_conflicts[0];
    assert_eq!(conflict.anchor_key, "neutral-anchor:shared-2");
    assert_eq!(
        conflict.canonical_ids,
        ["entity:two".to_string(), "entity:two-conflict".to_string()]
    );
    assert_eq!(conflict.source_rows.len(), 3);
    assert_eq!(artifact.consistency.abstentions.len(), 1);
    assert_eq!(
        artifact.consistency.abstentions[0].anchor_key,
        "neutral-anchor:shared-2"
    );
    assert!(
        artifact.consistency.abstentions[0]
            .message
            .contains("abstain")
    );
}

#[test]
fn two_source_link_is_a_strict_multisource_subset() {
    let fixture = MultiSourceFixture::new();
    let artifact = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("reference", EntitySourceRole::Reference, &fixture.alpha),
            source("target", EntitySourceRole::Target, &fixture.beta),
        ],
        comparison_graph: vec![EntitySourceComparison::new("reference", "target")],
        canonical_source: Some("reference"),
        default_pair_budget: 16,
        output_rows: &fixture.work_dir.join("two-source.csv"),
    })
    .expect("two-source subset materializes");

    assert_eq!(artifact.source_count, 2);
    assert_eq!(artifact.comparison_graph.len(), 1);
    assert_eq!(artifact.comparison_graph[0].candidate_pair_rows, 4);
    assert_eq!(
        artifact
            .sources
            .iter()
            .map(|source| (&source.name, source.role))
            .collect::<Vec<_>>(),
        vec![
            (&"reference".to_string(), EntitySourceRole::Reference),
            (&"target".to_string(), EntitySourceRole::Target),
        ]
    );

    let rows = fs::read_to_string(&artifact.materialized_rows_path).expect("two-source rows");
    assert!(rows.contains(",reference,reference,"));
    assert!(rows.contains(",target,target,"));
}

#[test]
fn duplicate_names_reserved_columns_and_unknown_edges_refuse_before_write() {
    let fixture = MultiSourceFixture::new();
    let duplicate = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("alpha", EntitySourceRole::Reference, &fixture.alpha),
            source("alpha", EntitySourceRole::Target, &fixture.beta),
        ],
        comparison_graph: vec![EntitySourceComparison::new("alpha", "beta")],
        canonical_source: None,
        default_pair_budget: 16,
        output_rows: &fixture.work_dir.join("duplicate.csv"),
    })
    .expect_err("duplicate names refuse");
    assert_eq!(duplicate.code, RefusalCode::EEntityInputContract);
    assert_eq!(duplicate.detail["reason"], "duplicate_source_name");

    let unknown = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("alpha", EntitySourceRole::Reference, &fixture.alpha),
            source("beta", EntitySourceRole::Target, &fixture.beta),
        ],
        comparison_graph: vec![EntitySourceComparison::new("alpha", "missing")],
        canonical_source: None,
        default_pair_budget: 16,
        output_rows: &fixture.work_dir.join("unknown.csv"),
    })
    .expect_err("unknown edge source refuses");
    assert_eq!(unknown.detail["reason"], "unknown_comparison_source");

    let reserved_path = fixture.work_dir.join("reserved.csv");
    fs::create_dir_all(&fixture.work_dir).expect("work dir exists");
    fs::write(
        &reserved_path,
        format!("{LINK_SOURCE_NAME_COLUMN},name\nbad,Reserved\n"),
    )
    .expect("reserved fixture");
    let reserved = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources: vec![
            source("alpha", EntitySourceRole::Reference, &fixture.alpha),
            source("reserved", EntitySourceRole::Target, &reserved_path),
        ],
        comparison_graph: vec![EntitySourceComparison::new("alpha", "reserved")],
        canonical_source: None,
        default_pair_budget: 16,
        output_rows: &fixture.work_dir.join("reserved-out.csv"),
    })
    .expect_err("reserved column refuses");
    assert_eq!(reserved.detail["reason"], "reserved_column");
    assert_eq!(reserved.detail["column"], LINK_SOURCE_NAME_COLUMN);
}

fn source<'a>(name: &'a str, role: EntitySourceRole, rows_path: &'a Path) -> EntityNamedSource<'a> {
    EntityNamedSource::new(name, role, rows_path).local_id_column("source_row_id")
}

fn anchored_source<'a>(
    name: &'a str,
    role: EntitySourceRole,
    rows_path: &'a Path,
) -> EntityNamedSource<'a> {
    source(name, role, rows_path).anchor("neutral-anchor", "anchor_id", "canonical_id")
}

fn strip_path(
    mut artifact: canon::entity::run::link::multisource::EntityMultisourceLinkArtifact,
) -> canon::entity::run::link::multisource::EntityMultisourceLinkArtifact {
    artifact.materialized_rows_path.clear();
    artifact
}

fn source_names_in_rows(rows: &str) -> BTreeSet<String> {
    let mut reader = csv::Reader::from_reader(rows.as_bytes());
    let headers = reader.headers().expect("headers").clone();
    let source_index = headers
        .iter()
        .position(|header| header == LINK_SOURCE_NAME_COLUMN)
        .expect("source name header");
    reader
        .records()
        .map(|record| {
            record
                .expect("record")
                .get(source_index)
                .expect("source name")
                .to_string()
        })
        .collect()
}

struct MultiSourceFixture {
    _temp: tempfile::TempDir,
    alpha: PathBuf,
    beta: PathBuf,
    gamma: PathBuf,
    work_dir: PathBuf,
}

impl MultiSourceFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let alpha = temp.path().join("alpha.csv");
        let beta = temp.path().join("beta.csv");
        let gamma = temp.path().join("gamma.csv");
        let work_dir = temp.path().join("work");

        fs::write(
            &alpha,
            "source_row_id,name,anchor_id,canonical_id\nalpha-1,Alpha One,shared-1,entity:one\nalpha-2,Alpha Two,shared-2,entity:two\n",
        )
        .expect("alpha rows");
        fs::write(
            &beta,
            "source_row_id,name,anchor_id,canonical_id\nbeta-1,Beta One,shared-1,entity:one\nbeta-2,Beta Two,shared-2,entity:two\n",
        )
        .expect("beta rows");
        fs::write(
            &gamma,
            "source_row_id,name,anchor_id,canonical_id\n gamma-1,Gamma One,shared-1,entity:one\ngamma-2,Gamma Two,shared-2,entity:two-conflict\n",
        )
        .expect("gamma rows");

        Self {
            _temp: temp,
            alpha,
            beta,
            gamma,
            work_dir,
        }
    }
}
