//! Promotion/write-back for `canon org`.

use super::{
    incumbent::load_incumbent_memory,
    types::{
        AliasMappingEntry, AuditArtifact, CANON_ORG_AUDIT_VERSION, CANON_ORG_PROMOTE_VERSION,
        CANON_ORG_RUN_VERSION, CANON_ORG_SOLVE_VERSION, CannotLinkFact, ContentAddressedArtifact,
        OrgEntityState, OrgError, OrgErrorCode, OrgResult, PendingClusterRecord, PromoteArtifact,
        PromotionDecision, PromotionRegistrySummary, PromotionWrites, SolveRunArtifact,
        TrustedAnchorRecord,
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const ORG_CANONICAL_TYPE: &str = "org_canon_id";

pub fn promote(
    result: &SolveRunArtifact,
    result_bytes: &[u8],
    audit: &AuditArtifact,
    audit_bytes: &[u8],
    registry_dir: &Path,
    next_version: &str,
) -> OrgResult<PromoteArtifact> {
    validate_result_artifact(result)?;
    validate_audit_artifact(result, result_bytes, audit, audit_bytes)?;
    validate_next_version(next_version)?;

    let before = load_incumbent_memory(registry_dir)?;
    validate_registry_snapshot(result, &before, next_version)?;

    let write_plan = build_write_plan(result, &before)?;
    let mapping_files = if write_plan.alias_entries.is_empty() {
        Vec::new()
    } else {
        vec![mapping_file_name(next_version)]
    };
    if result.proposed_registry_patch.mapping_files != mapping_files {
        return Err(promotion_error(
            "Promotion mapping files do not match the result artifact's proposed patch summary",
            json!({
                "expected_mapping_files": result.proposed_registry_patch.mapping_files,
                "actual_mapping_files": mapping_files,
            }),
        ));
    }
    apply_write_plan(
        registry_dir,
        next_version,
        &write_plan,
        before.alias_entries.len(),
    )?;
    let after = load_incumbent_memory(registry_dir)?;
    let mut writes = write_plan.summary.clone();
    writes.mapping_files = mapping_files;

    Ok(PromoteArtifact {
        version: CANON_ORG_PROMOTE_VERSION.to_string(),
        result: ContentAddressedArtifact {
            version: result.version.clone(),
            content_hash: blake3_string(result_bytes),
        },
        audit: ContentAddressedArtifact {
            version: audit.version.clone(),
            content_hash: blake3_string(audit_bytes),
        },
        registry: PromotionRegistrySummary {
            id: before.registry.id.clone(),
            version_before: before.registry.version.clone(),
            version_after: after.registry.version.clone(),
            source: before.registry.source.clone(),
            lookup_snapshot_hash_before: before.registry.lookup_snapshot_hash.clone(),
            escrow_snapshot_hash_before: before.registry.escrow_snapshot_hash.clone(),
            lookup_snapshot_hash_after: after.registry.lookup_snapshot_hash.clone(),
            escrow_snapshot_hash_after: after.registry.escrow_snapshot_hash.clone(),
        },
        decision: audit.summary.decision,
        writes,
    })
}

fn validate_result_artifact(result: &SolveRunArtifact) -> OrgResult<()> {
    match result.version.as_str() {
        CANON_ORG_RUN_VERSION | CANON_ORG_SOLVE_VERSION => Ok(()),
        other => Err(promotion_error(
            "Promotion requires a canon_org_run.v0 or canon_org_solve.v0 artifact",
            json!({
                "result_version": other,
            }),
        )),
    }
}

fn validate_audit_artifact(
    result: &SolveRunArtifact,
    result_bytes: &[u8],
    audit: &AuditArtifact,
    audit_bytes: &[u8],
) -> OrgResult<()> {
    if audit.version != CANON_ORG_AUDIT_VERSION {
        return Err(promotion_error(
            "Promotion requires a canon_org_audit.v0 artifact",
            json!({
                "audit_version": audit.version,
            }),
        ));
    }

    if !audit.summary.hard_gates_passed || audit.summary.decision != PromotionDecision::Promote {
        return Err(promotion_error(
            "Audit artifact did not approve promotion",
            json!({
                "hard_gates_passed": audit.summary.hard_gates_passed,
                "decision": audit.summary.decision,
                "gate_failures": audit.gate_failures,
            }),
        ));
    }

    let expected_result_hash = blake3_string(result_bytes);
    if audit.result.version != result.version
        || audit.result.content_hash != expected_result_hash
        || audit.result.strategy_content_hash != result.strategy.content_hash
        || audit.result.lookup_snapshot_hash != result.registry.lookup_snapshot_hash
        || audit.result.escrow_snapshot_hash != result.registry.escrow_snapshot_hash
    {
        return Err(promotion_error(
            "Audit artifact does not match the result artifact being promoted",
            json!({
                "expected": {
                    "version": result.version,
                    "content_hash": expected_result_hash,
                    "strategy_content_hash": result.strategy.content_hash,
                    "lookup_snapshot_hash": result.registry.lookup_snapshot_hash,
                    "escrow_snapshot_hash": result.registry.escrow_snapshot_hash,
                },
                "actual": audit.result,
            }),
        ));
    }

    let _audit_hash = blake3_string(audit_bytes);
    Ok(())
}

fn validate_next_version(next_version: &str) -> OrgResult<()> {
    if next_version.trim().is_empty() {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Promotion requires an explicit --next-version value",
            json!({
                "next_version": next_version,
            }),
        ));
    }

    Ok(())
}

fn validate_registry_snapshot(
    result: &SolveRunArtifact,
    before: &super::types::IncumbentMemory,
    next_version: &str,
) -> OrgResult<()> {
    if before.registry.id != result.registry.id
        || before.registry.version != result.registry.version
        || before.registry.lookup_snapshot_hash != result.registry.lookup_snapshot_hash
        || before.registry.escrow_snapshot_hash != result.registry.escrow_snapshot_hash
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Current registry snapshot is stale relative to the audited result artifact",
            json!({
                "expected": result.registry,
                "actual": before.registry,
            }),
        ));
    }

    if before.registry.version == next_version {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Promotion requires --next-version to differ from the current registry.json version",
            json!({
                "current_version": before.registry.version,
                "next_version": next_version,
            }),
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
struct WritePlan {
    alias_entries: Vec<AliasMappingEntry>,
    anchor_records: Vec<TrustedAnchorRecord>,
    pending_records: Vec<PendingClusterRecord>,
    cannot_link_records: Vec<CannotLinkFact>,
    summary: PromotionWrites,
}

fn build_write_plan(
    result: &SolveRunArtifact,
    before: &super::types::IncumbentMemory,
) -> OrgResult<WritePlan> {
    let existing_alias_keys = before
        .alias_entries
        .iter()
        .map(|entry| {
            (
                entry.input.clone(),
                entry.canonical_id.clone(),
                entry.canonical_type.clone(),
                entry.rule_id.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let existing_alias_by_input = before
        .alias_entries
        .iter()
        .map(|entry| (entry.input.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let existing_anchor_keys = before
        .trusted_anchors
        .iter()
        .map(|record| {
            (
                record.canonical_id.clone(),
                record.namespace.clone(),
                record.value.clone(),
            )
        })
        .collect::<BTreeSet<_>>();

    let mut alias_entries = Vec::new();
    let mut anchor_records = Vec::new();
    let mut new_entity_entries = 0u64;
    let mut existing_alias_entries = 0u64;
    let mut planned_alias_by_input = BTreeMap::<String, String>::new();

    for entity in &result.entities {
        let state_allowed = matches!(
            entity.state,
            OrgEntityState::PromotableNew | OrgEntityState::ResolvedExisting
        );
        if !state_allowed {
            continue;
        }

        let canonical_id = entity.canonical_id.clone().ok_or_else(|| {
            promotion_error(
                "Promotable entity is missing canonical_id",
                json!({
                    "state": entity.state,
                    "all_rows": entity.all_rows,
                }),
            )
        })?;

        let rule_id = format!("ORG_PROMOTION:{}", result.strategy.id);
        let mut entity_write_count = 0u64;

        for alias in &entity.eligible_writeback_aliases {
            if let Some(existing) = existing_alias_by_input.get(alias) {
                if existing.canonical_id == canonical_id
                    && existing.canonical_type == ORG_CANONICAL_TYPE
                {
                    continue;
                }

                return Err(promotion_error(
                    "Promotion would overwrite an existing alias mapping",
                    json!({
                        "input": alias,
                        "existing_canonical_id": existing.canonical_id,
                        "new_canonical_id": canonical_id,
                    }),
                ));
            }

            if let Some(existing_canonical_id) = planned_alias_by_input.get(alias) {
                if existing_canonical_id == &canonical_id {
                    continue;
                }

                return Err(promotion_error(
                    "Promotion would emit conflicting alias mappings in the same batch",
                    json!({
                        "input": alias,
                        "left_canonical_id": existing_canonical_id,
                        "right_canonical_id": canonical_id,
                    }),
                ));
            }

            let entry = AliasMappingEntry {
                input: alias.clone(),
                canonical_id: canonical_id.clone(),
                canonical_type: ORG_CANONICAL_TYPE.to_string(),
                rule_id: rule_id.clone(),
            };
            let key = (
                entry.input.clone(),
                entry.canonical_id.clone(),
                entry.canonical_type.clone(),
                entry.rule_id.clone(),
            );
            if existing_alias_keys.contains(&key) {
                continue;
            }
            if alias_entries.iter().any(|candidate: &AliasMappingEntry| {
                candidate.input == entry.input
                    && candidate.canonical_id == entry.canonical_id
                    && candidate.canonical_type == entry.canonical_type
                    && candidate.rule_id == entry.rule_id
            }) {
                continue;
            }

            alias_entries.push(entry);
            planned_alias_by_input.insert(alias.clone(), canonical_id.clone());
            entity_write_count += 1;
        }

        match entity.state {
            OrgEntityState::PromotableNew => new_entity_entries += entity_write_count,
            OrgEntityState::ResolvedExisting => existing_alias_entries += entity_write_count,
            _ => {}
        }

        for anchor in &entity.anchors {
            let record = TrustedAnchorRecord {
                canonical_id: canonical_id.clone(),
                namespace: anchor.namespace.clone(),
                value: anchor.value.clone(),
            };
            let key = (
                record.canonical_id.clone(),
                record.namespace.clone(),
                record.value.clone(),
            );
            if existing_anchor_keys.contains(&key) {
                continue;
            }
            anchor_records.push(record);
        }
    }

    alias_entries.sort_by(|left, right| {
        (
            &left.input,
            &left.canonical_id,
            &left.canonical_type,
            &left.rule_id,
        )
            .cmp(&(
                &right.input,
                &right.canonical_id,
                &right.canonical_type,
                &right.rule_id,
            ))
    });
    anchor_records.sort_by(|left, right| {
        (&left.canonical_id, &left.namespace, &left.value).cmp(&(
            &right.canonical_id,
            &right.namespace,
            &right.value,
        ))
    });
    anchor_records.dedup_by(|left, right| {
        left.canonical_id == right.canonical_id
            && left.namespace == right.namespace
            && left.value == right.value
    });

    let mut pending_records = before.pending_clusters.clone();
    let mut pending_updates = 0u64;
    for abstention in &result.abstentions {
        let Some(escrow) = &abstention.escrow else {
            continue;
        };
        if escrow.action != super::types::EscrowActionKind::UpsertPending {
            continue;
        }

        let escrow_id = escrow.escrow_id.clone().ok_or_else(|| {
            promotion_error(
                "Pending-cluster escrow action is missing escrow_id",
                json!({
                    "abstention_rows": abstention.all_rows,
                }),
            )
        })?;

        let record = PendingClusterRecord {
            escrow_id: escrow_id.clone(),
            profile: result.strategy.id.clone(),
            doc_ids: Vec::new(),
            surfaces: Vec::new(),
            anchors: Vec::new(),
            witness_pairs: Vec::new(),
            state: "pending".to_string(),
        };

        match pending_records
            .iter()
            .position(|candidate| candidate.escrow_id == escrow_id)
        {
            Some(index) => pending_records[index] = record,
            None => pending_records.push(record),
        }
        pending_updates += 1;
    }
    pending_records.sort_by(|left, right| left.escrow_id.cmp(&right.escrow_id));

    let mut cannot_link_records = before.cannot_link_facts.clone();
    let mut cannot_link_updates = 0u64;
    for abstention in &result.abstentions {
        let Some(escrow) = &abstention.escrow else {
            continue;
        };
        let Some(cannot_link) = &escrow.cannot_link else {
            continue;
        };
        if cannot_link_records.iter().any(|record| {
            record.left_key == cannot_link.left_key
                && record.right_key == cannot_link.right_key
                && record.reason == cannot_link.reason
        }) {
            continue;
        }
        cannot_link_records.push(cannot_link.clone());
        cannot_link_updates += 1;
    }
    cannot_link_records.sort_by(|left, right| {
        (&left.left_key, &left.right_key, &left.reason).cmp(&(
            &right.left_key,
            &right.right_key,
            &right.reason,
        ))
    });

    let summary = PromotionWrites {
        mapping_files: Vec::new(),
        new_entity_entries,
        existing_alias_entries,
        pending_cluster_entries: pending_updates,
        cannot_link_entries: cannot_link_updates,
    };

    if summary.new_entity_entries != result.proposed_registry_patch.new_entity_entries
        || summary.existing_alias_entries != result.proposed_registry_patch.existing_alias_entries
        || summary.pending_cluster_entries != result.proposed_escrow_patch.pending_cluster_entries
        || summary.cannot_link_entries != result.proposed_escrow_patch.cannot_link_entries
    {
        return Err(promotion_error(
            "Promotion write counts do not match the result artifact's proposed patch summary",
            json!({
                "expected_registry_patch": result.proposed_registry_patch,
                "expected_escrow_patch": result.proposed_escrow_patch,
                "actual_writes": summary,
            }),
        ));
    }

    Ok(WritePlan {
        alias_entries,
        anchor_records,
        pending_records,
        cannot_link_records,
        summary,
    })
}

fn apply_write_plan(
    registry_dir: &Path,
    next_version: &str,
    write_plan: &WritePlan,
    prior_alias_entry_count: usize,
) -> OrgResult<()> {
    fs::create_dir_all(registry_dir).map_err(io_promotion_error)?;

    let mapping_file_name = mapping_file_name(next_version);
    if !write_plan.alias_entries.is_empty() {
        let mapping_path = registry_dir.join(&mapping_file_name);
        write_json_pretty(&mapping_path, &write_plan.alias_entries)?;
    }

    if !write_plan.anchor_records.is_empty() {
        let anchors_dir = registry_dir.join("_anchors");
        fs::create_dir_all(&anchors_dir).map_err(io_promotion_error)?;
        let anchor_path = anchors_dir.join(format!("{}.anchors.jsonl", version_stem(next_version)));
        write_jsonl_file(&anchor_path, &write_plan.anchor_records)?;
    }

    let escrow_dir = registry_dir.join("_escrow");
    if !write_plan.pending_records.is_empty() || !write_plan.cannot_link_records.is_empty() {
        fs::create_dir_all(&escrow_dir).map_err(io_promotion_error)?;
    }
    if !write_plan.pending_records.is_empty() {
        write_jsonl_file(
            &escrow_dir.join("pending.jsonl"),
            &write_plan.pending_records,
        )?;
    }
    if !write_plan.cannot_link_records.is_empty() {
        write_jsonl_file(
            &escrow_dir.join("cannot_link.jsonl"),
            &write_plan.cannot_link_records,
        )?;
    }

    update_registry_json(
        &registry_dir.join("registry.json"),
        next_version,
        prior_alias_entry_count + write_plan.alias_entries.len(),
    )?;
    Ok(())
}

fn update_registry_json(path: &Path, next_version: &str, entry_count: usize) -> OrgResult<()> {
    let bytes = fs::read(path).map_err(io_promotion_error)?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        promotion_error(
            "registry.json is not valid JSON during promotion",
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    let object = value.as_object_mut().ok_or_else(|| {
        promotion_error(
            "registry.json must contain a JSON object during promotion",
            json!({
                "path": path.display().to_string(),
            }),
        )
    })?;
    object.insert(
        "version".to_string(),
        Value::String(next_version.to_string()),
    );
    object.insert("entry_count".to_string(), json!(entry_count));
    write_json_pretty(path, &value)
}

fn mapping_file_name(next_version: &str) -> String {
    format!("org-{}.json", version_stem(next_version))
}

fn version_stem(next_version: &str) -> String {
    let stem = next_version
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if stem.is_empty() {
        "next".to_string()
    } else {
        stem
    }
}

fn blake3_string(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> OrgResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        promotion_error(
            "Failed to serialize promotion JSON",
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    fs::write(path, bytes).map_err(io_promotion_error)
}

fn write_jsonl_file<T: Serialize>(path: &Path, records: &[T]) -> OrgResult<()> {
    let mut output = String::new();
    for record in records {
        output.push_str(&serde_json::to_string(record).map_err(|error| {
            promotion_error(
                "Failed to serialize promotion JSONL record",
                json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?);
        output.push('\n');
    }
    fs::write(path, output).map_err(io_promotion_error)
}

fn promotion_error(message: &str, detail: Value) -> OrgError {
    OrgError::with_detail(OrgErrorCode::Promotion, message, detail)
}

fn io_promotion_error(error: std::io::Error) -> OrgError {
    promotion_error(
        "Promotion file I/O failed",
        json!({ "error": error.to_string() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::types::{
        AbstentionRecord, AnchorValue, AuditMetrics, AuditSummary, EscrowActionKind,
        EscrowActionRecord, InheritanceMode, InheritanceRecord, RegistryPatchSummary,
        RegistrySnapshot, SolveRunSummary, SolvedEntity, StrategyReference, SuiteReference,
    };
    use serde::de::DeserializeOwned;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn read_jsonl_file<T: DeserializeOwned>(path: &Path) -> OrgResult<Vec<T>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let bytes = fs::read(path).map_err(io_promotion_error)?;
        let text = std::str::from_utf8(&bytes).map_err(|error| {
            promotion_error(
                "JSONL sidecar must be valid UTF-8",
                json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;

        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line).map_err(|error| {
                    promotion_error(
                        "Failed to parse JSONL sidecar record during promotion",
                        json!({
                            "path": path.display().to_string(),
                            "line": line,
                            "error": error.to_string(),
                        }),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn promote_writes_registry_and_escrow_sidecars() {
        let temp_dir = tempdir().expect("tempdir");
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.01");
        write_alias_file(
            temp_dir.path().join("aliases.json"),
            &[AliasMappingEntry {
                input: "Legacy Alias".to_string(),
                canonical_id: "IC-old".to_string(),
                canonical_type: ORG_CANONICAL_TYPE.to_string(),
                rule_id: "ORG_PROMOTION:bdc_org_graph.v1".to_string(),
            }],
        );

        let before = load_incumbent_memory(temp_dir.path()).expect("before snapshot");
        let result = positive_result(&before.registry);
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");
        let audit = matching_audit(&result, &result_bytes);
        let audit_bytes = serde_json::to_vec(&audit).expect("audit bytes");

        let artifact = promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "2026.03.02",
        )
        .expect("promotion to succeed");

        assert_eq!(artifact.registry.version_before, "2026.03.01");
        assert_eq!(artifact.registry.version_after, "2026.03.02");
        assert_eq!(artifact.writes.mapping_files, vec!["org-20260302.json"]);
        assert_eq!(artifact.writes.new_entity_entries, 2);
        assert_eq!(artifact.writes.existing_alias_entries, 1);
        assert_eq!(artifact.writes.pending_cluster_entries, 1);
        assert_eq!(artifact.writes.cannot_link_entries, 1);

        let mapping_entries: Vec<AliasMappingEntry> =
            serde_json::from_slice(&fs::read(temp_dir.path().join("org-20260302.json")).unwrap())
                .unwrap();
        assert_eq!(mapping_entries.len(), 3);
        assert_eq!(mapping_entries[0].canonical_type, ORG_CANONICAL_TYPE);

        let pending_records: Vec<PendingClusterRecord> =
            read_jsonl_file(&temp_dir.path().join("_escrow").join("pending.jsonl"))
                .expect("pending records");
        assert_eq!(pending_records.len(), 1);
        assert_eq!(pending_records[0].escrow_id, "OE-1");

        let cannot_link_records: Vec<CannotLinkFact> =
            read_jsonl_file(&temp_dir.path().join("_escrow").join("cannot_link.jsonl"))
                .expect("cannot-link records");
        assert_eq!(cannot_link_records.len(), 1);

        let anchor_files = fs::read_dir(temp_dir.path().join("_anchors"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(anchor_files.len(), 1);
        let anchor_records: Vec<TrustedAnchorRecord> =
            read_jsonl_file(&anchor_files[0]).expect("anchor records");
        assert_eq!(anchor_records.len(), 2);
    }

    #[test]
    fn promote_refuses_stale_registry_snapshot() {
        let temp_dir = tempdir().expect("tempdir");
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.01");
        let before = load_incumbent_memory(temp_dir.path()).expect("before snapshot");
        let mut result = positive_result(&before.registry);
        result.registry.lookup_snapshot_hash = "blake3:stale".to_string();
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");
        let audit = matching_audit(&result, &result_bytes);
        let audit_bytes = serde_json::to_vec(&audit).expect("audit bytes");

        let error = promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "2026.03.02",
        )
        .expect_err("stale registry to refuse");

        assert_eq!(error.code, OrgErrorCode::Promotion);
        assert!(error.message.contains("stale"));
    }

    #[test]
    fn promote_refuses_missing_or_unchanged_next_version() {
        let temp_dir = tempdir().expect("tempdir");
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.01");
        let before = load_incumbent_memory(temp_dir.path()).expect("before snapshot");
        let result = positive_result(&before.registry);
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");
        let audit = matching_audit(&result, &result_bytes);
        let audit_bytes = serde_json::to_vec(&audit).expect("audit bytes");

        let missing_version = promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "",
        )
        .expect_err("missing next-version to refuse");
        assert_eq!(missing_version.code, OrgErrorCode::Promotion);

        let unchanged_version = promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "2026.03.01",
        )
        .expect_err("unchanged next-version to refuse");
        assert_eq!(unchanged_version.code, OrgErrorCode::Promotion);
    }

    #[test]
    fn promote_refuses_alias_overwrite() {
        let temp_dir = tempdir().expect("tempdir");
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.01");
        write_alias_file(
            temp_dir.path().join("aliases.json"),
            &[AliasMappingEntry {
                input: "Acme Corp.".to_string(),
                canonical_id: "IC-conflict".to_string(),
                canonical_type: ORG_CANONICAL_TYPE.to_string(),
                rule_id: "ORG_PROMOTION:bdc_org_graph.v1".to_string(),
            }],
        );

        let before = load_incumbent_memory(temp_dir.path()).expect("before snapshot");
        let mut result = positive_result(&before.registry);
        result.entities[0].eligible_writeback_aliases = vec!["Acme Corp.".to_string()];
        result.proposed_registry_patch.new_entity_entries = 1;
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");
        let audit = matching_audit(&result, &result_bytes);
        let audit_bytes = serde_json::to_vec(&audit).expect("audit bytes");

        let error = promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "2026.03.02",
        )
        .expect_err("alias overwrite to refuse");

        assert_eq!(error.code, OrgErrorCode::Promotion);
        assert!(error.message.contains("overwrite"));
    }

    #[test]
    fn promote_skips_anchor_records_already_present_in_incumbent_memory() {
        let temp_dir = tempdir().expect("tempdir");
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.01");
        write_alias_file(
            temp_dir.path().join("aliases.json"),
            &[AliasMappingEntry {
                input: "Legacy Alias".to_string(),
                canonical_id: "IC-old".to_string(),
                canonical_type: ORG_CANONICAL_TYPE.to_string(),
                rule_id: "ORG_PROMOTION:bdc_org_graph.v1".to_string(),
            }],
        );
        fs::create_dir_all(temp_dir.path().join("_anchors")).unwrap();
        write_jsonl_file(
            &temp_dir.path().join("_anchors").join("existing.jsonl"),
            &[TrustedAnchorRecord {
                canonical_id: "IC-old".to_string(),
                namespace: "lei".to_string(),
                value: "549300OLD".to_string(),
            }],
        )
        .unwrap();

        let before = load_incumbent_memory(temp_dir.path()).expect("before snapshot");
        let result = positive_result(&before.registry);
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");
        let audit = matching_audit(&result, &result_bytes);
        let audit_bytes = serde_json::to_vec(&audit).expect("audit bytes");

        promote(
            &result,
            &result_bytes,
            &audit,
            &audit_bytes,
            temp_dir.path(),
            "2026.03.02",
        )
        .expect("promotion to succeed");

        let anchor_files = fs::read_dir(temp_dir.path().join("_anchors"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(anchor_files.len(), 2);

        let new_anchor_file = anchor_files
            .into_iter()
            .find(|path| path.file_name().unwrap() != "existing.jsonl")
            .expect("new anchor sidecar");
        let anchor_records: Vec<TrustedAnchorRecord> =
            read_jsonl_file(&new_anchor_file).expect("anchor records");
        assert_eq!(anchor_records.len(), 1);
        assert_eq!(anchor_records[0].canonical_id, "IC-new");
    }

    fn positive_result(registry: &RegistrySnapshot) -> SolveRunArtifact {
        SolveRunArtifact {
            version: CANON_ORG_RUN_VERSION.to_string(),
            strategy: StrategyReference {
                id: "bdc_org_graph.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:strategy".to_string(),
            },
            registry: registry.clone(),
            summary: SolveRunSummary {
                observations: 4,
                resolved_existing: 1,
                promotable_new: 1,
                abstain_low_evidence: 1,
                abstain_conflict: 1,
            },
            entities: vec![
                SolvedEntity {
                    state: OrgEntityState::PromotableNew,
                    canonical_id: Some("IC-new".to_string()),
                    aliases: vec!["Acme Corp.".to_string(), "ACME Corporation".to_string()],
                    anchors: vec![AnchorValue {
                        namespace: "lei".to_string(),
                        value: "549300AAA".to_string(),
                    }],
                    eligible_writeback_aliases: vec![
                        "Acme Corp.".to_string(),
                        "ACME Corporation".to_string(),
                    ],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::NoIncumbentOverlap,
                        incumbent_ids: Vec::new(),
                    },
                    ..SolvedEntity::default()
                },
                SolvedEntity {
                    state: OrgEntityState::ResolvedExisting,
                    canonical_id: Some("IC-old".to_string()),
                    aliases: vec!["Legacy Alias".to_string(), "Legacy Alias 2".to_string()],
                    anchors: vec![AnchorValue {
                        namespace: "lei".to_string(),
                        value: "549300OLD".to_string(),
                    }],
                    eligible_writeback_aliases: vec!["Legacy Alias 2".to_string()],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::SingleIncumbentOverlap,
                        incumbent_ids: vec!["IC-old".to_string()],
                    },
                    ..SolvedEntity::default()
                },
            ],
            abstentions: vec![
                AbstentionRecord {
                    state: OrgEntityState::AbstainLowEvidence,
                    all_rows: vec!["row-1".to_string(), "row-2".to_string()],
                    reason: "insufficient_distinct_docs".to_string(),
                    incumbent_ids: Vec::new(),
                    escrow: Some(EscrowActionRecord {
                        action: EscrowActionKind::UpsertPending,
                        escrow_id: Some("OE-1".to_string()),
                        cannot_link: None,
                    }),
                },
                AbstentionRecord {
                    state: OrgEntityState::AbstainConflict,
                    all_rows: vec!["row-3".to_string(), "row-4".to_string()],
                    reason: "trusted_anchor_conflict".to_string(),
                    incumbent_ids: vec!["IC-left".to_string(), "IC-right".to_string()],
                    escrow: Some(EscrowActionRecord {
                        action: EscrowActionKind::EmitCannotLink,
                        escrow_id: None,
                        cannot_link: Some(CannotLinkFact {
                            left_key: "lei:549300AAA".to_string(),
                            right_key: "lei:549300BBB".to_string(),
                            reason: "conflicting_trusted_anchor".to_string(),
                        }),
                    }),
                },
            ],
            contradictions: Vec::new(),
            proposed_registry_patch: RegistryPatchSummary {
                mapping_files: vec!["org-20260302.json".to_string()],
                new_entity_entries: 2,
                existing_alias_entries: 1,
            },
            proposed_escrow_patch: super::super::types::EscrowPatchSummary {
                pending_cluster_entries: 1,
                cannot_link_entries: 1,
            },
        }
    }

    fn matching_audit(result: &SolveRunArtifact, result_bytes: &[u8]) -> AuditArtifact {
        AuditArtifact {
            version: CANON_ORG_AUDIT_VERSION.to_string(),
            result: super::super::types::ResultReference {
                version: result.version.clone(),
                content_hash: blake3_string(result_bytes),
                strategy_content_hash: result.strategy.content_hash.clone(),
                lookup_snapshot_hash: result.registry.lookup_snapshot_hash.clone(),
                escrow_snapshot_hash: result.registry.escrow_snapshot_hash.clone(),
            },
            suite: SuiteReference {
                id: "bdc_org_eval.v1".to_string(),
            },
            summary: AuditSummary {
                decision: PromotionDecision::Promote,
                hard_gates_passed: true,
            },
            metrics: AuditMetrics {
                gold_pair_f1: Some(0.98),
                anchor_consistency: 1.0,
                anchor_conflicts: 0,
                holdout_score: 0.97,
                contradiction_rate: 0.0,
                perturbation_stability: 1.0,
                continuity_gain: 0.1,
                compression_gain: 0.2,
                registry_churn: 0.0,
                escrow_reuse_rate: 0.0,
            },
            gate_failures: Vec::new(),
        }
    }

    fn write_registry_json(path: &Path, id: &str, version: &str) {
        fs::write(
            path.join("registry.json"),
            serde_json::to_vec_pretty(&json!({
                "id": id,
                "version": version,
                "description": "test registry",
                "updated": "2026-03-24",
                "entry_count": 0,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_alias_file(path: PathBuf, entries: &[AliasMappingEntry]) {
        fs::write(path, serde_json::to_vec_pretty(entries).unwrap()).unwrap();
    }
}
