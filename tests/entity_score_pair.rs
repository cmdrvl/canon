#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        edge::EdgeEvidenceRecord,
        run::{EntityRunRequest, run_entity_workbench},
        score::ScoreLane,
        solve::{SolveArtifact, SolveReconciliationState},
    },
    sdk::{EntityScorePairRequest, EntityScorePairVerdict, score_entity_pair},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn sdk_score_pair_replays_full_run_edge_evidence_for_the_pair() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = write_support_rows(temp.path());
    let profile = write_support_profile(temp.path(), "9000", "1");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("run");
    write_support_registry(&registry, SupportRegistryMode::LeftOnly);

    run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: &profile,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("full run");

    let full_records: Vec<EdgeEvidenceRecord> =
        read_jsonl(&work_dir.join("evidence/evidence.jsonl"));
    assert_eq!(
        full_records.len(),
        1,
        "two-row fixture emits one candidate edge"
    );
    let full_record = full_records.into_iter().next().expect("one edge record");
    let solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    assert!(
        solve.entities.iter().any(|entity| {
            entity.state == SolveReconciliationState::ResolvedExisting
                && entity.canonical_id.as_deref() == Some("TNT-ACME-COFFEE")
                && entity.surface_ids.len() == 2
        }),
        "full run solver must merge the pair before the SDK banding claims would_merge"
    );

    let response = score_entity_pair(EntityScorePairRequest::v1(
        support_row("support:001", "Acme Coffee"),
        support_row("support:002", "Acme Coffee Shop"),
        profile.to_str().expect("profile path utf8"),
        profile.clone(),
    ))
    .expect("SDK score-pair");

    assert_eq!(response.evidence_record, full_record);
    assert_eq!(
        response.score_units,
        response.evidence_record.pair_score_total.as_u32()
    );
    assert_eq!(response.verdict, EntityScorePairVerdict::WouldMerge);
    assert_eq!(response.writes_performed, false);
    assert!(response.registry_snapshot_hash.is_none());
    assert!(
        response
            .evidence_waterfall
            .contributions
            .iter()
            .any(|contribution| contribution.operator == "string_similarity:tenant_core")
    );
    assert!(
        response
            .evidence_waterfall
            .contributions
            .iter()
            .any(|contribution| contribution.operator == "tfidf_cosine:tenant_tokens")
    );

    let swapped = score_entity_pair(EntityScorePairRequest::v1(
        support_row("support:002", "Acme Coffee Shop"),
        support_row("support:001", "Acme Coffee"),
        profile.to_str().expect("profile path utf8"),
        profile.clone(),
    ))
    .expect("SDK score-pair swapped");
    assert_eq!(swapped.evidence_record, response.evidence_record);
    assert_eq!(swapped.evidence_waterfall, response.evidence_waterfall);
    assert_eq!(swapped.verdict, response.verdict);
}

#[test]
fn sdk_score_pair_adds_registry_alias_support_when_registry_is_supplied() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile = write_support_profile(temp.path(), "10000", "10000");
    let registry = temp.path().join("registry");
    write_support_registry(&registry, SupportRegistryMode::BothSameCanonical);
    let tree_before = snapshot_tree(temp.path());

    let mut request = EntityScorePairRequest::v1(
        support_row("support:001", "Acme Coffee"),
        support_row("support:002", "Acme Coffee Shop"),
        profile.to_str().expect("profile path utf8"),
        profile,
    );
    request.registry = Some(registry);

    let response = score_entity_pair(request).expect("SDK score-pair with registry");
    assert_eq!(snapshot_tree(temp.path()), tree_before);
    assert_eq!(response.verdict, EntityScorePairVerdict::WouldMerge);
    assert!(response.registry_snapshot_hash.is_some());
    assert!(
        response
            .evidence_record
            .hits
            .iter()
            .any(|hit| hit.lane == ScoreLane::Support && hit.operator_id == "registry_alias_match")
    );
}

#[test]
fn sdk_score_pair_registry_conflict_vetoes_otherwise_similar_pair() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile = write_support_profile(temp.path(), "9000", "1");
    let registry = temp.path().join("registry");
    write_support_registry(&registry, SupportRegistryMode::ConflictingCanonicals);

    let mut request = EntityScorePairRequest::v1(
        support_row("support:001", "Acme Coffee"),
        support_row("support:002", "Acme Coffee Shop"),
        profile.to_str().expect("profile path utf8"),
        profile,
    );
    request.registry = Some(registry);

    let response = score_entity_pair(request).expect("SDK score-pair with conflicting registry");
    assert_eq!(response.verdict, EntityScorePairVerdict::CannotLink);
    assert!(response.evidence_record.has_hard_cannot_link);
    assert!(
        response.evidence_record.hits.iter().any(|hit| {
            hit.lane == ScoreLane::AntiMerge
                && hit.operator_id == "registry_alias_conflict"
                && hit.hard_cannot_link
        }),
        "registry conflict must be a hard anti-merge hit, not a weak score deduction"
    );
}

#[test]
fn sdk_score_pair_no_support_hits_yields_empty_waterfall_and_zero_score() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile = write_support_profile(temp.path(), "10000", "10000");

    let response = score_entity_pair(EntityScorePairRequest::v1(
        support_row("support:001", "Northpoint Books"),
        support_row("support:002", "Westside Florist"),
        profile.to_str().expect("profile path utf8"),
        profile,
    ))
    .expect("SDK score-pair without support");

    assert_eq!(response.score_units, 0);
    assert_eq!(response.verdict, EntityScorePairVerdict::BelowFloor);
    assert!(response.evidence_waterfall.contributions.is_empty());
    assert!(
        response
            .evidence_record
            .hits
            .iter()
            .all(|hit| hit.lane != ScoreLane::Support),
        "a no-support pair must not fabricate support evidence"
    );
}

#[test]
fn sdk_score_pair_malformed_record_refuses_without_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile = write_support_profile(temp.path(), "9000", "1");

    let refusal = score_entity_pair(EntityScorePairRequest::v1(
        json!(["not", "an", "object"]),
        support_row("support:002", "Acme Coffee Shop"),
        profile.to_str().expect("profile path utf8"),
        profile,
    ))
    .expect_err("malformed left record refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityInputContract);
    assert_ne!(refusal.envelope.outcome, canon::Outcome::Resolved);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SupportRegistryMode {
    LeftOnly,
    BothSameCanonical,
    ConflictingCanonicals,
}

fn support_row(source_row_id: &str, raw_tenant_name: &str) -> Value {
    json!({
        "source_row_id": source_row_id,
        "deal_id": "D001",
        "loan_id": "L001",
        "property_id": "P001",
        "raw_tenant_name": raw_tenant_name
    })
}

fn write_support_rows(base: &Path) -> PathBuf {
    let rows = base.join("support_rows.csv");
    fs::write(
        &rows,
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n\
support:001,D001,L001,P001,Acme Coffee\n\
support:002,D002,L002,P002,Acme Coffee Shop\n",
    )
    .expect("support rows");
    rows
}

fn write_support_profile(base: &Path, string_threshold: &str, tfidf_threshold: &str) -> PathBuf {
    let profile = base.join("support_profile.yaml");
    fs::write(
        &profile,
        format!(
            r#"profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields:
  - source_row_id
  - deal_id
  - loan_id
  - property_id
  - raw_tenant_name
normalized_views:
  tenant_core:
    operators:
      - unicode_fold
      - lowercase
      - strip_tenant_noise
      - strip_legal_suffixes
      - normalize_whitespace
  tenant_tokens:
    operators:
      - unicode_fold
      - lowercase
      - tokenize
      - drop_tenant_stopwords
  tenant_brand:
    operators:
      - unicode_fold
      - lowercase
      - tenant_brand_fingerprint
      - normalize_whitespace
evidence:
  support:
    - op: exact_view
      view: tenant_core
    - op: string_similarity
      view: tenant_core
      params:
        metric: jaro_winkler
        min_score_units: "{string_threshold}"
    - op: tfidf_cosine
      view: tenant_tokens
      params:
        min_score_units: "{tfidf_threshold}"
        top_k: "10"
        candidate_cap: "10"
  cannot_link:
    - op: protected_token_conflict
      view: tenant_tokens
  relation_hints:
    - op: related_brand_family
      view: tenant_brand
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
solver:
  backbone_score_min: "9000"
  attach_score_min: "7000"
  abstain_margin: "500"
"#,
        ),
    )
    .expect("support profile");
    profile
}

fn write_support_registry(registry: &Path, mode: SupportRegistryMode) {
    fs::create_dir_all(registry).expect("registry dir");
    let aliases = match mode {
        SupportRegistryMode::LeftOnly => {
            vec![json!({
                "input": "Acme Coffee",
                "canonical_id": "TNT-ACME-COFFEE",
                "canonical_type": "tenant_label",
                "rule_id": "TEST_ALIAS"
            })]
        }
        SupportRegistryMode::BothSameCanonical => {
            vec![
                json!({
                    "input": "Acme Coffee",
                    "canonical_id": "TNT-ACME-COFFEE",
                    "canonical_type": "tenant_label",
                    "rule_id": "TEST_ALIAS"
                }),
                json!({
                    "input": "Acme Coffee Shop",
                    "canonical_id": "TNT-ACME-COFFEE",
                    "canonical_type": "tenant_label",
                    "rule_id": "TEST_ALIAS"
                }),
            ]
        }
        SupportRegistryMode::ConflictingCanonicals => {
            vec![
                json!({
                    "input": "Acme Coffee",
                    "canonical_id": "TNT-ACME-COFFEE",
                    "canonical_type": "tenant_label",
                    "rule_id": "TEST_ALIAS"
                }),
                json!({
                    "input": "Acme Coffee Shop",
                    "canonical_id": "TNT-ACME-COFFEE-SHOP",
                    "canonical_type": "tenant_label",
                    "rule_id": "TEST_ALIAS"
                }),
            ]
        }
    };
    fs::write(
        registry.join("registry.json"),
        format!(
            r#"{{"id":"support-tenants","version":"2026.07.12","description":"Support edge test registry","updated":"2026-07-12","entry_count":{}}}"#,
            aliases.len()
        ),
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&aliases).expect("aliases json"),
    )
    .expect("aliases");
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl bytes")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl record parses"))
        .collect()
}

fn snapshot_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    snapshot_tree_inner(root, root, &mut files);
    files
}

fn snapshot_tree_inner(root: &Path, current: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries = fs::read_dir(current)
        .expect("read tree")
        .map(|entry| entry.expect("tree entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            snapshot_tree_inner(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("path under root")
                .to_string_lossy()
                .into_owned();
            files.insert(relative, fs::read(path).expect("file bytes"));
        }
    }
}
