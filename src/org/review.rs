//! Human adjudication export/import for org identity review queues.

use super::{
    incumbent::load_incumbent_memory,
    types::{
        AliasMappingEntry, AnchorValue, AuditArtifact, CANON_ORG_AUDIT_VERSION, CannotLinkFact,
        OrgEntityState, OrgError, OrgErrorCode, OrgResult, PendingClusterRecord, PromotionDecision,
        PromotionWrites, RegistrySnapshot, RowPair, SolveRunArtifact, TrustedAnchorRecord,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const ORG_CANONICAL_TYPE: &str = "org_canon_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewInclude {
    Resolved,
    Escrow,
    Contradictions,
    All,
}

impl ReviewInclude {
    fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Escrow => "escrow",
            Self::Contradictions => "contradictions",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewResultReference {
    pub version: String,
    pub content_hash: String,
    pub strategy_content_hash: String,
    pub lookup_snapshot_hash: String,
    pub escrow_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub items: usize,
    pub resolved: usize,
    pub escrow: usize,
    pub contradictions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidenceScore {
    pub left_row_id: String,
    pub right_row_id: String,
    pub pair_score_total: i64,
    #[serde(default)]
    pub pair_score_by_namespace: BTreeMap<String, i64>,
    #[serde(default)]
    pub operator_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub review_id: String,
    pub category: String,
    pub state: OrgEntityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default)]
    pub source_row_ids: Vec<String>,
    #[serde(default)]
    pub observed_names: Vec<String>,
    #[serde(default)]
    pub anchors: Vec<AnchorValue>,
    #[serde(default)]
    pub incumbent_ids: Vec<String>,
    #[serde(default)]
    pub evidence_scores: Vec<ReviewEvidenceScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contradiction_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_key: Option<String>,
    pub proposed_action: String,
    #[serde(default = "default_decision")]
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewExportOutput {
    pub version: String,
    pub include: String,
    pub result: ReviewResultReference,
    pub strategy_id: String,
    pub registry: RegistrySnapshot,
    pub summary: ReviewSummary,
    pub items: Vec<ReviewItem>,
}

impl ReviewExportOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{} review export {} | items={} resolved={} escrow={} contradictions={}",
            self.strategy_id,
            self.include,
            self.summary.items,
            self.summary.resolved,
            self.summary.escrow,
            self.summary.contradictions,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportReference {
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportSummary {
    pub reviewed_items: usize,
    pub accepted_alias_items: usize,
    pub pending_items: usize,
    pub cannot_link_items: usize,
    pub skipped_items: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportOutput {
    pub version: String,
    pub review: ReviewImportReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit: Option<ReviewImportReference>,
    pub registry: ReviewImportRegistrySummary,
    pub summary: ReviewImportSummary,
    pub writes: PromotionWrites,
    pub proof_hashes: ReviewProofHashes,
}

impl ReviewImportOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{} review import {} -> {} | reviewed={} aliases={} pending={} cannot_link={}",
            self.registry.id,
            self.registry.version_before,
            self.registry.version_after,
            self.summary.reviewed_items,
            self.writes.new_entity_entries + self.writes.existing_alias_entries,
            self.writes.pending_cluster_entries,
            self.writes.cannot_link_entries,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportRegistrySummary {
    pub id: String,
    pub version_before: String,
    pub version_after: String,
    pub source: String,
    pub lookup_snapshot_hash_before: String,
    pub escrow_snapshot_hash_before: String,
    pub lookup_snapshot_hash_after: String,
    pub escrow_snapshot_hash_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewProofHashes {
    pub review_input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_input: Option<String>,
    pub alias_patch: String,
    pub anchor_patch: String,
    pub escrow_patch: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CsvReviewRow {
    review_id: String,
    category: String,
    state: OrgEntityState,
    canonical_id: String,
    source_row_ids: String,
    observed_names: String,
    anchors: String,
    incumbent_ids: String,
    evidence_scores: String,
    contradiction_reason: String,
    left_key: String,
    right_key: String,
    proposed_action: String,
    decision: String,
    result_version: String,
    result_content_hash: String,
    strategy_content_hash: String,
    registry_id: String,
    registry_version: String,
    lookup_snapshot_hash: String,
    escrow_snapshot_hash: String,
    strategy_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct CsvReviewRowOut<'a> {
    review_id: &'a str,
    category: &'a str,
    state: OrgEntityState,
    canonical_id: &'a str,
    source_row_ids: String,
    observed_names: String,
    anchors: String,
    incumbent_ids: String,
    evidence_scores: String,
    contradiction_reason: &'a str,
    left_key: &'a str,
    right_key: &'a str,
    proposed_action: &'a str,
    decision: &'a str,
    result_version: &'a str,
    result_content_hash: &'a str,
    strategy_content_hash: &'a str,
    registry_id: &'a str,
    registry_version: &'a str,
    lookup_snapshot_hash: &'a str,
    escrow_snapshot_hash: &'a str,
    strategy_id: &'a str,
}

#[derive(Debug, Clone, Default)]
struct ReviewWritePlan {
    alias_entries: Vec<AliasMappingEntry>,
    anchor_records: Vec<TrustedAnchorRecord>,
    pending_records: Vec<PendingClusterRecord>,
    cannot_link_records: Vec<CannotLinkFact>,
    summary: ReviewImportSummary,
    writes: PromotionWrites,
}

pub fn export(
    result: &SolveRunArtifact,
    result_bytes: &[u8],
    include: ReviewInclude,
) -> OrgResult<ReviewExportOutput> {
    let result_hash = blake3_string(result_bytes);
    let mut items = Vec::new();

    if matches!(include, ReviewInclude::Resolved | ReviewInclude::All) {
        for entity in &result.entities {
            items.push(review_item_for_entity(&result_hash, entity));
        }
    }

    if matches!(include, ReviewInclude::Escrow | ReviewInclude::All) {
        for abstention in &result.abstentions {
            items.push(review_item_for_abstention(&result_hash, abstention));
        }
    }

    if matches!(include, ReviewInclude::Contradictions | ReviewInclude::All) {
        for contradiction in &result.contradictions {
            items.push(review_item_for_contradiction(&result_hash, contradiction));
        }
    }

    items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
    let summary = summarize_review_items(&items);

    Ok(ReviewExportOutput {
        version: "canon_org_review_export.v0".to_string(),
        include: include.as_str().to_string(),
        result: ReviewResultReference {
            version: result.version.clone(),
            content_hash: result_hash,
            strategy_content_hash: result.strategy.content_hash.clone(),
            lookup_snapshot_hash: result.registry.lookup_snapshot_hash.clone(),
            escrow_snapshot_hash: result.registry.escrow_snapshot_hash.clone(),
        },
        strategy_id: result.strategy.id.clone(),
        registry: result.registry.clone(),
        summary,
        items,
    })
}

pub fn export_csv(output: &ReviewExportOutput) -> OrgResult<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for item in &output.items {
        writer
            .serialize(CsvReviewRowOut {
                review_id: &item.review_id,
                category: &item.category,
                state: item.state,
                canonical_id: item.canonical_id.as_deref().unwrap_or(""),
                source_row_ids: json_cell(&item.source_row_ids)?,
                observed_names: json_cell(&item.observed_names)?,
                anchors: json_cell(&item.anchors)?,
                incumbent_ids: json_cell(&item.incumbent_ids)?,
                evidence_scores: json_cell(&item.evidence_scores)?,
                contradiction_reason: item.contradiction_reason.as_deref().unwrap_or(""),
                left_key: item.left_key.as_deref().unwrap_or(""),
                right_key: item.right_key.as_deref().unwrap_or(""),
                proposed_action: &item.proposed_action,
                decision: &item.decision,
                result_version: &output.result.version,
                result_content_hash: &output.result.content_hash,
                strategy_content_hash: &output.result.strategy_content_hash,
                registry_id: &output.registry.id,
                registry_version: &output.registry.version,
                lookup_snapshot_hash: &output.registry.lookup_snapshot_hash,
                escrow_snapshot_hash: &output.registry.escrow_snapshot_hash,
                strategy_id: &output.strategy_id,
            })
            .map_err(csv_error)?;
    }
    let bytes = writer.into_inner().map_err(|error| {
        OrgError::with_detail(
            OrgErrorCode::ArtifactContract,
            "Failed to finalize org review CSV",
            json!({ "error": error.to_string() }),
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        OrgError::with_detail(
            OrgErrorCode::ArtifactContract,
            "Org review CSV must be valid UTF-8",
            json!({ "error": error.to_string() }),
        )
    })
}

pub fn import(
    review_path: &Path,
    review_bytes: &[u8],
    registry_dir: &Path,
    next_version: &str,
    audit: Option<(&AuditArtifact, &[u8])>,
) -> OrgResult<ReviewImportOutput> {
    validate_next_version(next_version)?;
    let review = parse_review(review_path, review_bytes)?;
    validate_review(&review)?;

    let before = load_incumbent_memory(registry_dir)?;
    validate_registry_snapshot(&review, &before, next_version)?;
    validate_audit_if_required(&review, audit)?;
    let write_plan = build_write_plan(&review, &before, next_version)?;
    apply_write_plan(
        registry_dir,
        next_version,
        &review,
        &write_plan,
        before.alias_entries.len(),
    )?;
    let after = load_incumbent_memory(registry_dir)?;

    let proof_hashes = ReviewProofHashes {
        review_input: blake3_string(review_bytes),
        audit_input: audit.map(|(_, bytes)| blake3_string(bytes)),
        alias_patch: blake3_json(&write_plan.alias_entries)?,
        anchor_patch: blake3_json(&write_plan.anchor_records)?,
        escrow_patch: blake3_json(&json!({
            "pending": write_plan.pending_records,
            "cannot_link": write_plan.cannot_link_records,
        }))?,
    };
    let writes = write_plan.writes.clone();
    let summary = write_plan.summary.clone();
    write_review_proofs(
        registry_dir,
        next_version,
        review_bytes,
        audit.map(|(_, bytes)| bytes),
        &proof_hashes,
    )?;

    Ok(ReviewImportOutput {
        version: "canon_org_review_import.v0".to_string(),
        review: ReviewImportReference {
            version: review.version,
            content_hash: blake3_string(review_bytes),
        },
        audit: audit.map(|(audit, bytes)| ReviewImportReference {
            version: audit.version.clone(),
            content_hash: blake3_string(bytes),
        }),
        registry: ReviewImportRegistrySummary {
            id: before.registry.id,
            version_before: before.registry.version,
            version_after: after.registry.version,
            source: before.registry.source,
            lookup_snapshot_hash_before: before.registry.lookup_snapshot_hash,
            escrow_snapshot_hash_before: before.registry.escrow_snapshot_hash,
            lookup_snapshot_hash_after: after.registry.lookup_snapshot_hash,
            escrow_snapshot_hash_after: after.registry.escrow_snapshot_hash,
        },
        summary,
        writes,
        proof_hashes,
    })
}

fn parse_review(path: &Path, bytes: &[u8]) -> OrgResult<ReviewExportOutput> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => parse_review_csv(path, bytes),
        _ => serde_json::from_slice(bytes).map_err(|error| {
            OrgError::with_detail(
                OrgErrorCode::ArtifactContract,
                "Failed to parse org review JSON artifact",
                json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        }),
    }
}

fn parse_review_csv(path: &Path, bytes: &[u8]) -> OrgResult<ReviewExportOutput> {
    let mut reader = csv::Reader::from_reader(bytes);
    let mut rows = Vec::new();
    for row in reader.deserialize::<CsvReviewRow>() {
        rows.push(row.map_err(csv_error)?);
    }
    let Some(first) = rows.first() else {
        return Err(review_error(
            "Review CSV must contain at least one row",
            json!({ "path": path.display().to_string() }),
        ));
    };

    let registry = RegistrySnapshot {
        id: first.registry_id.clone(),
        version: first.registry_version.clone(),
        source: String::new(),
        lookup_snapshot_hash: first.lookup_snapshot_hash.clone(),
        escrow_snapshot_hash: first.escrow_snapshot_hash.clone(),
    };
    let result = ReviewResultReference {
        version: first.result_version.clone(),
        content_hash: first.result_content_hash.clone(),
        strategy_content_hash: first.strategy_content_hash.clone(),
        lookup_snapshot_hash: first.lookup_snapshot_hash.clone(),
        escrow_snapshot_hash: first.escrow_snapshot_hash.clone(),
    };
    let strategy_id = first.strategy_id.clone();
    let mut items = Vec::new();
    for row in rows {
        if row.result_content_hash != result.content_hash
            || row.result_version != result.version
            || row.strategy_content_hash != result.strategy_content_hash
            || row.registry_id != registry.id
            || row.registry_version != registry.version
            || row.lookup_snapshot_hash != registry.lookup_snapshot_hash
            || row.escrow_snapshot_hash != registry.escrow_snapshot_hash
            || row.strategy_id != strategy_id
        {
            return Err(review_error(
                "Review CSV rows disagree on result or registry metadata",
                json!({ "review_id": row.review_id }),
            ));
        }
        items.push(ReviewItem {
            review_id: row.review_id,
            category: row.category,
            state: row.state,
            canonical_id: optional_string(row.canonical_id),
            source_row_ids: parse_json_cell(&row.source_row_ids, "source_row_ids")?,
            observed_names: parse_json_cell(&row.observed_names, "observed_names")?,
            anchors: parse_json_cell(&row.anchors, "anchors")?,
            incumbent_ids: parse_json_cell(&row.incumbent_ids, "incumbent_ids")?,
            evidence_scores: parse_json_cell(&row.evidence_scores, "evidence_scores")?,
            contradiction_reason: optional_string(row.contradiction_reason),
            left_key: optional_string(row.left_key),
            right_key: optional_string(row.right_key),
            proposed_action: row.proposed_action,
            decision: row.decision,
        });
    }
    let summary = summarize_review_items(&items);

    Ok(ReviewExportOutput {
        version: "canon_org_review_export.v0".to_string(),
        include: "csv".to_string(),
        result,
        strategy_id,
        registry,
        summary,
        items,
    })
}

fn review_item_for_entity(result_hash: &str, entity: &super::types::SolvedEntity) -> ReviewItem {
    let category = "resolved";
    let key = format!(
        "{}:{}:{}",
        category,
        entity.canonical_id.as_deref().unwrap_or(""),
        entity.all_rows.join("|")
    );
    ReviewItem {
        review_id: stable_review_id(result_hash, category, &key),
        category: category.to_string(),
        state: entity.state,
        canonical_id: entity.canonical_id.clone(),
        source_row_ids: entity.all_rows.clone(),
        observed_names: sorted_strings(entity.aliases.clone()),
        anchors: sorted_anchors(entity.anchors.clone()),
        incumbent_ids: entity.inheritance.incumbent_ids.clone(),
        evidence_scores: entity
            .merge_witnesses
            .iter()
            .map(|witness| ReviewEvidenceScore {
                left_row_id: witness.left_row_id.clone(),
                right_row_id: witness.right_row_id.clone(),
                pair_score_total: witness.pair_score_total,
                pair_score_by_namespace: witness.pair_score_by_namespace.clone(),
                operator_ids: witness.operator_ids.clone(),
            })
            .collect(),
        contradiction_reason: None,
        left_key: None,
        right_key: None,
        proposed_action: "accept_aliases".to_string(),
        decision: default_decision(),
    }
}

fn review_item_for_abstention(
    result_hash: &str,
    abstention: &super::types::AbstentionRecord,
) -> ReviewItem {
    let category = "escrow";
    let action = match abstention
        .escrow
        .as_ref()
        .map(|escrow| escrow.action)
        .unwrap_or_default()
    {
        super::types::EscrowActionKind::UpsertPending => "create_pending",
        super::types::EscrowActionKind::EmitCannotLink => "emit_cannot_link",
    };
    let (left_key, right_key) = abstention
        .escrow
        .as_ref()
        .and_then(|escrow| escrow.cannot_link.as_ref())
        .map(|fact| (Some(fact.left_key.clone()), Some(fact.right_key.clone())))
        .unwrap_or((None, None));
    let key = format!(
        "{}:{}:{}",
        category,
        abstention.reason,
        abstention.all_rows.join("|")
    );

    ReviewItem {
        review_id: stable_review_id(result_hash, category, &key),
        category: category.to_string(),
        state: abstention.state,
        canonical_id: None,
        source_row_ids: abstention.all_rows.clone(),
        observed_names: Vec::new(),
        anchors: Vec::new(),
        incumbent_ids: abstention.incumbent_ids.clone(),
        evidence_scores: Vec::new(),
        contradiction_reason: Some(abstention.reason.clone()),
        left_key,
        right_key,
        proposed_action: action.to_string(),
        decision: default_decision(),
    }
}

fn review_item_for_contradiction(
    result_hash: &str,
    contradiction: &super::types::ContradictionRecord,
) -> ReviewItem {
    let category = "contradiction";
    let key = format!(
        "{}:{}:{}:{}",
        category,
        contradiction.reason,
        contradiction.left_key.as_deref().unwrap_or(""),
        contradiction.right_key.as_deref().unwrap_or("")
    );
    ReviewItem {
        review_id: stable_review_id(result_hash, category, &key),
        category: category.to_string(),
        state: OrgEntityState::AbstainConflict,
        canonical_id: None,
        source_row_ids: contradiction.row_ids.clone(),
        observed_names: Vec::new(),
        anchors: Vec::new(),
        incumbent_ids: Vec::new(),
        evidence_scores: Vec::new(),
        contradiction_reason: Some(contradiction.reason.clone()),
        left_key: contradiction.left_key.clone(),
        right_key: contradiction.right_key.clone(),
        proposed_action: "emit_cannot_link".to_string(),
        decision: default_decision(),
    }
}

fn validate_review(review: &ReviewExportOutput) -> OrgResult<()> {
    if review.version != "canon_org_review_export.v0" {
        return Err(review_error(
            "Review import requires canon_org_review_export.v0",
            json!({ "version": review.version }),
        ));
    }
    let mut seen = BTreeSet::new();
    for item in &review.items {
        if item.review_id.trim().is_empty() {
            return Err(review_error(
                "Review item is missing review_id",
                json!({ "item": item }),
            ));
        }
        if !seen.insert(item.review_id.clone()) {
            return Err(review_error(
                "Duplicate review_id in review artifact",
                json!({ "review_id": item.review_id }),
            ));
        }
        let decision = normalize_decision(&item.decision);
        if !matches!(
            decision.as_str(),
            "undecided"
                | "defer"
                | "reject"
                | "noop"
                | "accept_aliases"
                | "create_pending"
                | "emit_cannot_link"
        ) {
            return Err(review_error(
                "Review item has unsupported decision",
                json!({
                    "review_id": item.review_id,
                    "decision": item.decision,
                }),
            ));
        }
    }
    Ok(())
}

fn validate_registry_snapshot(
    review: &ReviewExportOutput,
    before: &super::types::IncumbentMemory,
    next_version: &str,
) -> OrgResult<()> {
    if before.registry.id != review.registry.id
        || before.registry.version != review.registry.version
        || before.registry.lookup_snapshot_hash != review.registry.lookup_snapshot_hash
        || before.registry.escrow_snapshot_hash != review.registry.escrow_snapshot_hash
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Current registry snapshot is stale relative to the review artifact",
            json!({
                "expected": review.registry,
                "actual": before.registry,
            }),
        ));
    }

    if before.registry.version == next_version {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import requires --next-version to differ from the current registry.json version",
            json!({
                "current_version": before.registry.version,
                "next_version": next_version,
            }),
        ));
    }
    Ok(())
}

fn validate_audit_if_required(
    review: &ReviewExportOutput,
    audit: Option<(&AuditArtifact, &[u8])>,
) -> OrgResult<()> {
    let requires_audit = review
        .items
        .iter()
        .any(|item| normalize_decision(&item.decision) == "accept_aliases");
    if !requires_audit {
        return Ok(());
    }
    let Some((audit, _audit_bytes)) = audit else {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import requires --audit for alias or anchor promotion decisions",
            json!({ "required_for_decision": "accept_aliases" }),
        ));
    };
    if audit.version != CANON_ORG_AUDIT_VERSION
        || !audit.summary.hard_gates_passed
        || audit.summary.decision != PromotionDecision::Promote
        || audit.result.version != review.result.version
        || audit.result.content_hash != review.result.content_hash
        || audit.result.strategy_content_hash != review.result.strategy_content_hash
        || audit.result.lookup_snapshot_hash != review.result.lookup_snapshot_hash
        || audit.result.escrow_snapshot_hash != review.result.escrow_snapshot_hash
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Audit artifact does not authorize review promotion decisions",
            json!({
                "review_result": review.result,
                "audit_result": audit.result,
                "audit_version": audit.version,
                "hard_gates_passed": audit.summary.hard_gates_passed,
                "decision": audit.summary.decision,
            }),
        ));
    }
    Ok(())
}

fn build_write_plan(
    review: &ReviewExportOutput,
    before: &super::types::IncumbentMemory,
    next_version: &str,
) -> OrgResult<ReviewWritePlan> {
    let mut plan = ReviewWritePlan::default();
    let mut planned_alias_by_input = BTreeMap::<String, String>::new();
    let mut planned_anchor_by_key = BTreeMap::<(String, String), String>::new();
    let mut new_entity_entries = 0u64;
    let mut existing_alias_entries = 0u64;
    let existing_alias_by_input = before
        .alias_entries
        .iter()
        .map(|entry| (entry.input.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let existing_anchor_by_key = before
        .trusted_anchors
        .iter()
        .map(|record| ((record.namespace.clone(), record.value.clone()), record))
        .collect::<BTreeMap<_, _>>();

    let mut pending_records = before.pending_clusters.clone();
    let mut cannot_link_records = before.cannot_link_facts.clone();

    for item in &review.items {
        plan.summary.reviewed_items += 1;
        match normalize_decision(&item.decision).as_str() {
            "accept_aliases" => {
                let canonical_id = required_canonical_id(item)?;
                let alias_count_before = plan.alias_entries.len();
                for alias in &item.observed_names {
                    validate_alias_write(
                        item,
                        alias,
                        &canonical_id,
                        &existing_alias_by_input,
                        &planned_alias_by_input,
                    )?;
                    if existing_alias_by_input.contains_key(alias) {
                        continue;
                    }
                    if planned_alias_by_input
                        .insert(alias.clone(), canonical_id.clone())
                        .is_none()
                    {
                        plan.alias_entries.push(AliasMappingEntry {
                            input: alias.clone(),
                            canonical_id: canonical_id.clone(),
                            canonical_type: ORG_CANONICAL_TYPE.to_string(),
                            rule_id: format!("ORG_REVIEW:{}", item.review_id),
                        });
                    }
                }
                for anchor in &item.anchors {
                    validate_anchor_write(
                        item,
                        anchor,
                        &canonical_id,
                        &existing_anchor_by_key,
                        &planned_anchor_by_key,
                    )?;
                    planned_anchor_by_key.insert(
                        (anchor.namespace.clone(), anchor.value.clone()),
                        canonical_id.clone(),
                    );
                    let record = TrustedAnchorRecord {
                        canonical_id: canonical_id.clone(),
                        namespace: anchor.namespace.clone(),
                        value: anchor.value.clone(),
                    };
                    if !plan
                        .anchor_records
                        .iter()
                        .any(|candidate| candidate == &record)
                        && !before
                            .trusted_anchors
                            .iter()
                            .any(|candidate| candidate == &record)
                    {
                        plan.anchor_records.push(record);
                    }
                }
                let added_aliases = plan.alias_entries.len().saturating_sub(alias_count_before);
                if item.state == OrgEntityState::ResolvedExisting {
                    existing_alias_entries += added_aliases as u64;
                } else {
                    new_entity_entries += added_aliases as u64;
                }
                plan.summary.accepted_alias_items += 1;
            }
            "create_pending" => {
                let escrow_id = pending_review_id(item);
                let record = PendingClusterRecord {
                    escrow_id: escrow_id.clone(),
                    profile: review.strategy_id.clone(),
                    doc_ids: Vec::new(),
                    surfaces: item.observed_names.clone(),
                    anchors: item.anchors.clone(),
                    witness_pairs: witness_pairs_for_item(item),
                    state: "pending".to_string(),
                };
                match pending_records
                    .iter()
                    .position(|candidate| candidate.escrow_id == escrow_id)
                {
                    Some(index) => pending_records[index] = record,
                    None => pending_records.push(record),
                }
                plan.summary.pending_items += 1;
            }
            "emit_cannot_link" => {
                let fact = cannot_link_for_item(item)?;
                if !cannot_link_records.iter().any(|record| record == &fact) {
                    cannot_link_records.push(fact);
                }
                plan.summary.cannot_link_items += 1;
            }
            _ => plan.summary.skipped_items += 1,
        }
    }

    plan.alias_entries.sort_by(|left, right| {
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
    plan.anchor_records.sort_by(|left, right| {
        (&left.canonical_id, &left.namespace, &left.value).cmp(&(
            &right.canonical_id,
            &right.namespace,
            &right.value,
        ))
    });
    pending_records.sort_by(|left, right| left.escrow_id.cmp(&right.escrow_id));
    cannot_link_records.sort_by(|left, right| {
        (&left.left_key, &left.right_key, &left.reason).cmp(&(
            &right.left_key,
            &right.right_key,
            &right.reason,
        ))
    });
    plan.writes = PromotionWrites {
        mapping_files: (!plan.alias_entries.is_empty())
            .then(|| default_mapping_file_name(next_version))
            .into_iter()
            .collect(),
        new_entity_entries,
        existing_alias_entries,
        pending_cluster_entries: pending_records
            .len()
            .saturating_sub(before.pending_clusters.len()) as u64,
        cannot_link_entries: cannot_link_records
            .len()
            .saturating_sub(before.cannot_link_facts.len()) as u64,
    };
    plan.pending_records = pending_records;
    plan.cannot_link_records = cannot_link_records;
    Ok(plan)
}

fn apply_write_plan(
    registry_dir: &Path,
    next_version: &str,
    review: &ReviewExportOutput,
    plan: &ReviewWritePlan,
    prior_alias_count: usize,
) -> OrgResult<()> {
    fs::create_dir_all(registry_dir).map_err(io_review_error)?;
    let mapping_file_name = default_mapping_file_name(next_version);

    if !plan.alias_entries.is_empty() {
        write_json_pretty(&registry_dir.join(&mapping_file_name), &plan.alias_entries)?;
    }
    if !plan.anchor_records.is_empty() {
        let anchors_dir = registry_dir.join("_anchors");
        fs::create_dir_all(&anchors_dir).map_err(io_review_error)?;
        write_jsonl_file(
            &anchors_dir.join(format!("{}.anchors.jsonl", review_stem(next_version))),
            &plan.anchor_records,
        )?;
    }
    if plan.writes.pending_cluster_entries > 0 || plan.writes.cannot_link_entries > 0 {
        fs::create_dir_all(registry_dir.join("_escrow")).map_err(io_review_error)?;
    }
    if plan.writes.pending_cluster_entries > 0 {
        write_jsonl_file(
            &registry_dir.join("_escrow/pending.jsonl"),
            &plan.pending_records,
        )?;
    }
    if plan.writes.cannot_link_entries > 0 {
        write_jsonl_file(
            &registry_dir.join("_escrow/cannot_link.jsonl"),
            &plan.cannot_link_records,
        )?;
    }
    update_registry_json(
        &registry_dir.join("registry.json"),
        next_version,
        prior_alias_count + plan.alias_entries.len(),
    )?;

    let _ = review;
    Ok(())
}

fn validate_alias_write(
    item: &ReviewItem,
    alias: &str,
    canonical_id: &str,
    existing_alias_by_input: &BTreeMap<String, &AliasMappingEntry>,
    planned_alias_by_input: &BTreeMap<String, String>,
) -> OrgResult<()> {
    if alias.trim().is_empty() {
        return Err(review_error(
            "Review alias promotion contains an empty alias",
            json!({ "review_id": item.review_id }),
        ));
    }
    if let Some(existing) = existing_alias_by_input.get(alias)
        && existing.canonical_id != canonical_id
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import would overwrite an existing alias mapping",
            json!({
                "review_id": item.review_id,
                "input": alias,
                "existing_canonical_id": existing.canonical_id,
                "new_canonical_id": canonical_id,
            }),
        ));
    }
    if let Some(existing_canonical_id) = planned_alias_by_input.get(alias)
        && existing_canonical_id != canonical_id
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import would emit conflicting alias mappings in one batch",
            json!({
                "review_id": item.review_id,
                "input": alias,
                "left_canonical_id": existing_canonical_id,
                "right_canonical_id": canonical_id,
            }),
        ));
    }
    Ok(())
}

fn validate_anchor_write(
    item: &ReviewItem,
    anchor: &AnchorValue,
    canonical_id: &str,
    existing_anchor_by_key: &BTreeMap<(String, String), &TrustedAnchorRecord>,
    planned_anchor_by_key: &BTreeMap<(String, String), String>,
) -> OrgResult<()> {
    if anchor.namespace.trim().is_empty() || anchor.value.trim().is_empty() {
        return Err(review_error(
            "Review anchor promotion contains an incomplete anchor",
            json!({ "review_id": item.review_id }),
        ));
    }
    if let Some(existing) =
        existing_anchor_by_key.get(&(anchor.namespace.clone(), anchor.value.clone()))
        && existing.canonical_id != canonical_id
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import would create a trusted-anchor conflict",
            json!({
                "review_id": item.review_id,
                "namespace": anchor.namespace,
                "value": anchor.value,
                "existing_canonical_id": existing.canonical_id,
                "new_canonical_id": canonical_id,
            }),
        ));
    }
    if let Some(existing_canonical_id) =
        planned_anchor_by_key.get(&(anchor.namespace.clone(), anchor.value.clone()))
        && existing_canonical_id != canonical_id
    {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import would emit conflicting trusted anchors in one batch",
            json!({
                "review_id": item.review_id,
                "namespace": anchor.namespace,
                "value": anchor.value,
                "left_canonical_id": existing_canonical_id,
                "right_canonical_id": canonical_id,
            }),
        ));
    }
    Ok(())
}

fn required_canonical_id(item: &ReviewItem) -> OrgResult<String> {
    item.canonical_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            review_error(
                "Review alias promotion requires canonical_id",
                json!({ "review_id": item.review_id }),
            )
        })
}

fn cannot_link_for_item(item: &ReviewItem) -> OrgResult<CannotLinkFact> {
    let (left_key, right_key) = match (item.left_key.as_deref(), item.right_key.as_deref()) {
        (Some(left), Some(right)) if !left.trim().is_empty() && !right.trim().is_empty() => {
            (left.to_string(), right.to_string())
        }
        _ if item.incumbent_ids.len() >= 2 => {
            (item.incumbent_ids[0].clone(), item.incumbent_ids[1].clone())
        }
        _ => {
            return Err(review_error(
                "Review cannot-link decision requires left/right keys or two incumbent_ids",
                json!({ "review_id": item.review_id }),
            ));
        }
    };
    if left_key == right_key {
        return Err(review_error(
            "Review cannot-link decision cannot link a key to itself",
            json!({ "review_id": item.review_id, "key": left_key }),
        ));
    }
    Ok(CannotLinkFact {
        left_key,
        right_key,
        reason: item
            .contradiction_reason
            .clone()
            .unwrap_or_else(|| format!("review:{}", item.review_id)),
    })
}

fn witness_pairs_for_item(item: &ReviewItem) -> Vec<RowPair> {
    let mut rows = item.source_row_ids.clone();
    rows.sort();
    rows.windows(2)
        .map(|pair| RowPair {
            left_row_id: pair[0].clone(),
            right_row_id: pair[1].clone(),
        })
        .collect()
}

fn summarize_review_items(items: &[ReviewItem]) -> ReviewSummary {
    ReviewSummary {
        items: items.len(),
        resolved: items
            .iter()
            .filter(|item| item.category == "resolved")
            .count(),
        escrow: items
            .iter()
            .filter(|item| item.category == "escrow")
            .count(),
        contradictions: items
            .iter()
            .filter(|item| item.category == "contradiction")
            .count(),
    }
}

fn normalize_decision(decision: &str) -> String {
    decision.trim().to_ascii_lowercase()
}

fn stable_review_id(result_hash: &str, category: &str, key: &str) -> String {
    let digest = blake3::hash(format!("{result_hash}\n{category}\n{key}").as_bytes());
    format!("rvw-{}", &digest.to_hex()[..16])
}

fn pending_review_id(item: &ReviewItem) -> String {
    format!("OE-{}", item.review_id.trim_start_matches("rvw-"))
}

fn default_mapping_file_name(next_version: &str) -> String {
    format!("org-review-{}.json", review_stem(next_version))
}

fn review_stem(next_version: &str) -> String {
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

fn update_registry_json(path: &Path, next_version: &str, entry_count: usize) -> OrgResult<()> {
    let bytes = fs::read(path).map_err(io_review_error)?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        review_error(
            "registry.json is not valid JSON during review import",
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    let object = value.as_object_mut().ok_or_else(|| {
        review_error(
            "registry.json must contain a JSON object during review import",
            json!({ "path": path.display().to_string() }),
        )
    })?;
    object.insert(
        "version".to_string(),
        Value::String(next_version.to_string()),
    );
    object.insert("entry_count".to_string(), json!(entry_count));
    write_json_pretty(path, &value)
}

fn write_review_proofs(
    registry_dir: &Path,
    next_version: &str,
    review_bytes: &[u8],
    audit_bytes: Option<&[u8]>,
    proof_hashes: &ReviewProofHashes,
) -> OrgResult<()> {
    let dir = registry_dir.join("_reviews");
    fs::create_dir_all(&dir).map_err(io_review_error)?;
    let stem = review_stem(next_version);
    fs::write(dir.join(format!("{stem}.review.json")), review_bytes).map_err(io_review_error)?;
    if let Some(audit_bytes) = audit_bytes {
        fs::write(dir.join(format!("{stem}.audit.json")), audit_bytes).map_err(io_review_error)?;
    }
    write_json_pretty(&dir.join(format!("{stem}.proof.json")), proof_hashes)
}

fn validate_next_version(next_version: &str) -> OrgResult<()> {
    if next_version.trim().is_empty() {
        return Err(OrgError::with_detail(
            OrgErrorCode::Promotion,
            "Review import requires an explicit --next-version value",
            json!({ "next_version": next_version }),
        ));
    }
    Ok(())
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn sorted_anchors(mut values: Vec<AnchorValue>) -> Vec<AnchorValue> {
    values.sort_by(|left, right| {
        (&left.namespace, &left.value).cmp(&(&right.namespace, &right.value))
    });
    values.dedup_by(|left, right| left.namespace == right.namespace && left.value == right.value);
    values
}

fn optional_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn default_decision() -> String {
    "undecided".to_string()
}

fn json_cell<T: Serialize>(value: &T) -> OrgResult<String> {
    serde_json::to_string(value).map_err(|error| {
        OrgError::with_detail(
            OrgErrorCode::ArtifactContract,
            "Failed to serialize org review CSV cell",
            json!({ "error": error.to_string() }),
        )
    })
}

fn parse_json_cell<T: for<'de> Deserialize<'de>>(raw: &str, field: &str) -> OrgResult<T> {
    serde_json::from_str(raw).map_err(|error| {
        OrgError::with_detail(
            OrgErrorCode::ArtifactContract,
            "Failed to parse org review CSV JSON cell",
            json!({
                "field": field,
                "error": error.to_string(),
            }),
        )
    })
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> OrgResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        review_error(
            "Failed to serialize review JSON",
            json!({ "path": path.display().to_string(), "error": error.to_string() }),
        )
    })?;
    fs::write(path, bytes).map_err(io_review_error)
}

fn write_jsonl_file<T: Serialize>(path: &Path, records: &[T]) -> OrgResult<()> {
    let mut output = String::new();
    for record in records {
        output.push_str(&serde_json::to_string(record).map_err(|error| {
            review_error(
                "Failed to serialize review JSONL record",
                json!({ "path": path.display().to_string(), "error": error.to_string() }),
            )
        })?);
        output.push('\n');
    }
    fs::write(path, output).map_err(io_review_error)
}

fn blake3_json<T: Serialize>(value: &T) -> OrgResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        review_error(
            "Failed to serialize review proof hash input",
            json!({ "error": error.to_string() }),
        )
    })?;
    Ok(blake3_string(&bytes))
}

fn blake3_string(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn csv_error(error: csv::Error) -> OrgError {
    OrgError::with_detail(
        OrgErrorCode::ArtifactContract,
        "Org review CSV parse or write failed",
        json!({ "error": error.to_string() }),
    )
}

fn review_error(message: &str, detail: Value) -> OrgError {
    OrgError::with_detail(OrgErrorCode::Promotion, message, detail)
}

fn io_review_error(error: std::io::Error) -> OrgError {
    review_error(
        "Review import file I/O failed",
        json!({ "error": error.to_string() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::types::{
        AbstentionRecord, AuditMetrics, AuditSummary, CANON_ORG_AUDIT_VERSION,
        CANON_ORG_RUN_VERSION, EscrowActionKind, EscrowActionRecord, InheritanceMode,
        InheritanceRecord, RegistryPatchSummary, SolveRunSummary, StrategyReference,
        SuiteReference,
    };
    use serde::de::DeserializeOwned;
    use tempfile::{TempDir, tempdir};

    fn write_registry(dir: &Path) {
        fs::write(
            dir.join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": "org-test",
                "version": "1.0.0",
                "description": "test",
                "updated": "2026-05-06",
                "entry_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn result_fixture(registry_dir: &Path) -> (SolveRunArtifact, Vec<u8>) {
        let memory = load_incumbent_memory(registry_dir).unwrap();
        let result = SolveRunArtifact {
            version: CANON_ORG_RUN_VERSION.to_string(),
            strategy: StrategyReference {
                id: "bdc_org_graph.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:strategy".to_string(),
            },
            registry: memory.registry,
            summary: SolveRunSummary {
                observations: 3,
                resolved_existing: 0,
                promotable_new: 1,
                abstain_low_evidence: 1,
                abstain_conflict: 0,
            },
            entities: vec![super::super::types::SolvedEntity {
                state: OrgEntityState::PromotableNew,
                canonical_id: Some("ORG-1".to_string()),
                backbone_rows: vec!["row-1".to_string(), "row-2".to_string()],
                attached_rows: Vec::new(),
                all_rows: vec!["row-1".to_string(), "row-2".to_string()],
                aliases: vec!["Acme Inc".to_string(), "Acme Incorporated".to_string()],
                anchors: vec![AnchorValue {
                    namespace: "lei".to_string(),
                    value: "LEI1".to_string(),
                }],
                merge_witnesses: vec![super::super::types::MergeWitness {
                    left_row_id: "row-1".to_string(),
                    right_row_id: "row-2".to_string(),
                    pair_score_total: 9,
                    pair_score_by_namespace: BTreeMap::from([("name".to_string(), 9)]),
                    operator_ids: vec!["exact_view:core_name".to_string()],
                }],
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::NoIncumbentOverlap,
                    incumbent_ids: Vec::new(),
                },
                eligible_writeback_aliases: vec!["Acme Inc".to_string()],
                escrow: None,
            }],
            abstentions: vec![AbstentionRecord {
                state: OrgEntityState::AbstainLowEvidence,
                all_rows: vec!["row-3".to_string()],
                reason: "single_doc_without_unique_anchor".to_string(),
                incumbent_ids: Vec::new(),
                escrow: Some(EscrowActionRecord {
                    action: EscrowActionKind::UpsertPending,
                    escrow_id: Some("OE-1".to_string()),
                    cannot_link: None,
                }),
            }],
            contradictions: vec![super::super::types::ContradictionRecord {
                reason: "trusted_anchor_conflict".to_string(),
                row_ids: vec!["row-4".to_string(), "row-5".to_string()],
                left_key: Some("lei:LEI4".to_string()),
                right_key: Some("lei:LEI5".to_string()),
            }],
            proposed_registry_patch: RegistryPatchSummary {
                mapping_files: Vec::new(),
                new_entity_entries: 1,
                existing_alias_entries: 0,
            },
            proposed_escrow_patch: super::super::types::EscrowPatchSummary {
                pending_cluster_entries: 1,
                cannot_link_entries: 0,
            },
        };
        let bytes = serde_json::to_vec(&result).unwrap();
        (result, bytes)
    }

    fn matching_audit(result: &SolveRunArtifact, result_bytes: &[u8]) -> (AuditArtifact, Vec<u8>) {
        let audit = AuditArtifact {
            version: CANON_ORG_AUDIT_VERSION.to_string(),
            result: super::super::types::ResultReference {
                version: result.version.clone(),
                content_hash: blake3_string(result_bytes),
                strategy_content_hash: result.strategy.content_hash.clone(),
                lookup_snapshot_hash: result.registry.lookup_snapshot_hash.clone(),
                escrow_snapshot_hash: result.registry.escrow_snapshot_hash.clone(),
            },
            suite: SuiteReference {
                id: "suite".to_string(),
            },
            summary: AuditSummary {
                decision: PromotionDecision::Promote,
                hard_gates_passed: true,
            },
            metrics: AuditMetrics::default(),
            gate_failures: Vec::new(),
        };
        let bytes = serde_json::to_vec(&audit).unwrap();
        (audit, bytes)
    }

    fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn export_is_deterministic_and_includes_review_context() {
        let dir = tempdir().unwrap();
        write_registry(dir.path());
        let (result, bytes) = result_fixture(dir.path());

        let first = export(&result, &bytes, ReviewInclude::All).unwrap();
        let second = export(&result, &bytes, ReviewInclude::All).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.summary.resolved, 1);
        assert_eq!(first.summary.escrow, 1);
        assert_eq!(first.summary.contradictions, 1);
        assert!(
            first
                .items
                .iter()
                .any(|item| item.proposed_action == "accept_aliases"
                    && item.observed_names.contains(&"Acme Inc".to_string()))
        );
    }

    #[test]
    fn import_clean_review_writes_alias_anchor_and_escrow_patches() {
        let dir = tempdir().unwrap();
        write_registry(dir.path());
        let (result, bytes) = result_fixture(dir.path());
        let mut review = export(&result, &bytes, ReviewInclude::All).unwrap();
        for item in &mut review.items {
            item.decision = item.proposed_action.clone();
        }
        let review_bytes = serde_json::to_vec(&review).unwrap();
        let review_path = dir.path().join("review.json");
        let (audit, audit_bytes) = matching_audit(&result, &bytes);

        let output = import(
            &review_path,
            &review_bytes,
            dir.path(),
            "2.0.0",
            Some((&audit, &audit_bytes)),
        )
        .unwrap();

        assert_eq!(output.registry.version_after, "2.0.0");
        assert_eq!(output.writes.new_entity_entries, 2);
        assert_eq!(output.writes.pending_cluster_entries, 1);
        assert_eq!(output.writes.cannot_link_entries, 1);
        let aliases: Vec<AliasMappingEntry> =
            serde_json::from_slice(&fs::read(dir.path().join("org-review-200.json")).unwrap())
                .unwrap();
        assert_eq!(aliases.len(), 2);
        let anchors: Vec<TrustedAnchorRecord> =
            read_jsonl(&dir.path().join("_anchors/200.anchors.jsonl"));
        assert_eq!(anchors.len(), 1);
        let pending: Vec<PendingClusterRecord> =
            read_jsonl(&dir.path().join("_escrow/pending.jsonl"));
        assert_eq!(pending.len(), 1);
        let cannot_link: Vec<CannotLinkFact> =
            read_jsonl(&dir.path().join("_escrow/cannot_link.jsonl"));
        assert_eq!(cannot_link.len(), 1);
    }

    #[test]
    fn import_refuses_anchor_conflict() {
        let dir = tempdir().unwrap();
        write_registry(dir.path());
        fs::create_dir(dir.path().join("_anchors")).unwrap();
        fs::write(
            dir.path().join("_anchors/existing.jsonl"),
            "{\"canonical_id\":\"ORG-OTHER\",\"namespace\":\"lei\",\"value\":\"LEI1\"}\n",
        )
        .unwrap();
        let (result, bytes) = result_fixture(dir.path());
        let mut review = export(&result, &bytes, ReviewInclude::Resolved).unwrap();
        review.items[0].decision = "accept_aliases".to_string();
        let review_bytes = serde_json::to_vec(&review).unwrap();
        let (audit, audit_bytes) = matching_audit(&result, &bytes);

        let error = import(
            &dir.path().join("review.json"),
            &review_bytes,
            dir.path(),
            "2.0.0",
            Some((&audit, &audit_bytes)),
        )
        .unwrap_err();

        assert!(error.message.contains("trusted-anchor conflict"));
    }

    #[test]
    fn import_refuses_stale_registry() {
        let dir = tempdir().unwrap();
        write_registry(dir.path());
        let (result, bytes) = result_fixture(dir.path());
        let mut review = export(&result, &bytes, ReviewInclude::Escrow).unwrap();
        review.items[0].decision = "create_pending".to_string();
        let review_bytes = serde_json::to_vec(&review).unwrap();
        fs::write(
            dir.path().join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": "org-test",
                "version": "1.1.0",
                "description": "test",
                "updated": "2026-05-06",
                "entry_count": 0
            }))
            .unwrap(),
        )
        .unwrap();

        let error = import(
            &dir.path().join("review.json"),
            &review_bytes,
            dir.path(),
            "2.0.0",
            None,
        )
        .unwrap_err();

        assert!(error.message.contains("stale"));
    }

    #[test]
    fn csv_round_trip_imports_review_decisions() {
        let dir = tempdir().unwrap();
        write_registry(dir.path());
        let (result, bytes) = result_fixture(dir.path());
        let mut review = export(&result, &bytes, ReviewInclude::Escrow).unwrap();
        review.items[0].decision = "create_pending".to_string();
        let csv = export_csv(&review).unwrap();

        let output = import(
            &dir.path().join("review.csv"),
            csv.as_bytes(),
            dir.path(),
            "2.0.0",
            None,
        )
        .unwrap();

        assert_eq!(output.summary.pending_items, 1);
        assert_eq!(output.writes.pending_cluster_entries, 1);
    }

    #[allow(dead_code)]
    fn _keep_tempdir(_: &TempDir) {}
}
