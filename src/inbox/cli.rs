#![forbid(unsafe_code)]

use crate::{
    CanonOutput, RefusalCode,
    cli::{
        InboxApplyReviewCli, InboxCli, InboxEntityPlanMode, InboxExplainCli, InboxExportReviewCli,
        InboxListCli, InboxPlanEntityCli, InboxShowCli, InboxStatsCli, InboxSubcommand,
        RegistryEmitMode,
    },
    refusal,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use super::{
    InboxError, UnresolvedInboxArtifact, UnresolvedInboxItem, finalize_artifact,
    group::{
        GroupReviewAction, GroupReviewPatch, UnresolvedGroupingPlan, group_unresolved_artifact,
    },
    rank::{InboxPriorityRankingArtifact, PriorityPolicy, RankedInboxItem, rank_inbox},
};

const LIST_SCHEMA_VERSION: &str = "canon.inbox.list.v1";
const SHOW_SCHEMA_VERSION: &str = "canon.inbox.show.v1";
const EXPLAIN_SCHEMA_VERSION: &str = "canon.inbox.explain.v1";
const STATS_SCHEMA_VERSION: &str = "canon.inbox.stats.v1";
const REVIEW_EXPORT_SCHEMA_VERSION: &str = "canon.inbox.review_export.v1";
const REVIEW_APPLY_SCHEMA_VERSION: &str = "canon.inbox.review_apply.v1";
const ENTITY_PLAN_SCHEMA_VERSION: &str = "canon.inbox.entity_plan.v1";
const NO_IDENTITY_STATUS: &str = "no_identity_decision";

pub fn run(inbox: &InboxCli) -> Result<u8, Box<dyn Error>> {
    match &inbox.command {
        InboxSubcommand::List(args) => run_list(args),
        InboxSubcommand::Show(args) => run_show(args),
        InboxSubcommand::Explain(args) => run_explain(args),
        InboxSubcommand::Stats(args) => run_stats(args),
        InboxSubcommand::ExportReview(args) => run_export_review(args),
        InboxSubcommand::ApplyReview(args) => run_apply_review(args),
        InboxSubcommand::PlanEntity(args) => run_plan_entity(args),
    }
}

#[derive(Debug, Clone)]
struct LoadedInbox {
    path: PathBuf,
    artifact: UnresolvedInboxArtifact,
    ranking: InboxPriorityRankingArtifact,
}

#[derive(Debug, Clone, Default, Serialize)]
struct InboxFilters {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    event_kind: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reason_code: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    field_role: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    partition: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PageInfo {
    limit: usize,
    cursor: Option<String>,
    next_cursor: Option<String>,
    total_filtered: usize,
    returned: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ListItem {
    rank: u64,
    event_key: String,
    event_kind: String,
    reason_code: String,
    field_name: String,
    field_role: String,
    expected_coverage_value_units: i64,
    queue_partition: String,
    occurrence_count: u64,
    uncertainty_flags: Vec<String>,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ListReport {
    schema_version: &'static str,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: &'static str,
    filters: InboxFilters,
    page: PageInfo,
    items: Vec<ListItem>,
}

#[derive(Debug, Serialize)]
struct ShowReport {
    schema_version: &'static str,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: &'static str,
    event_key: String,
    item: UnresolvedInboxItem,
    ranked_item: RankedInboxItem,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ExplainReport {
    schema_version: &'static str,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: &'static str,
    event_key: String,
    score: RankedInboxItem,
    provenance: InboxProvenance,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct InboxProvenance {
    field_name: String,
    field_role: String,
    event_kind: String,
    reason_code: String,
    occurrence_count: u64,
    first_seen_at: String,
    last_seen_at: String,
    source_refs: Vec<String>,
    namespace_hints: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StatsReport {
    schema_version: &'static str,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: &'static str,
    inbox_summary: super::InboxSummary,
    ranking_summary: super::rank::PriorityRankingSummary,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReviewExportArtifact {
    schema_version: String,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: String,
    page: ReviewPage,
    decisions: Vec<ReviewDecision>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReviewPage {
    limit: usize,
    cursor: Option<String>,
    next_cursor: Option<String>,
    total_filtered: usize,
    returned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewDecision {
    review_id: String,
    action: GroupReviewAction,
    member_event_keys: Vec<String>,
    operator_ref: String,
    reason: String,
    reviewed_at: String,
    identity_status: String,
}

#[derive(Debug, Serialize)]
struct ReviewApplyReceipt {
    schema_version: &'static str,
    source_inbox_artifact_hash: String,
    review_path: String,
    output_path: String,
    identity_status: &'static str,
    applied_decision_count: usize,
    grouped_artifact_hash: String,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct EntityPlanArtifact {
    schema_version: String,
    source_inbox_artifact_hash: String,
    ranking_artifact_hash: String,
    identity_status: String,
    mode: String,
    bounded_selection: EntityPlanBounds,
    selected_items: Vec<EntityPlanItem>,
    request: EntityWorkbenchRequest,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct EntityPlanBounds {
    max_items: usize,
    selected_count: usize,
    selection_rule: String,
}

#[derive(Debug, Serialize)]
struct EntityPlanItem {
    event_key: String,
    rank: u64,
    field_name: String,
    field_role: String,
    reason_code: String,
    occurrence_refs: Vec<super::InboxOccurrenceRef>,
}

#[derive(Debug, Serialize)]
struct EntityWorkbenchRequest {
    command_family: &'static str,
    requested_mode: String,
    identity_decision: &'static str,
    required_inputs: Vec<&'static str>,
    candidate_event_keys: Vec<String>,
    preview_command: String,
}

fn run_list(args: &InboxListCli) -> Result<u8, Box<dyn Error>> {
    let limit = match checked_limit(args.limit, &args.emit) {
        Ok(limit) => limit,
        Err(exit) => return Ok(exit),
    };
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    let filters = InboxFilters {
        event_kind: normalize_filter_values(&args.event_kind),
        reason_code: normalize_filter_values(&args.reason_code),
        field_role: normalize_filter_values(&args.field_role),
        partition: normalize_filter_values(&args.partition),
    };
    let (ranked, page) =
        match page_ranked_items(&loaded, &filters, args.cursor.as_deref(), limit, &args.emit) {
            Ok(page) => page,
            Err(exit) => return Ok(exit),
        };
    let items = ranked
        .iter()
        .map(|ranked| {
            let item = item_by_key(&loaded.artifact, &ranked.event_key)
                .expect("ranked items always reference inbox items");
            ListItem {
                rank: ranked.rank,
                event_key: ranked.event_key.clone(),
                event_kind: enum_name(&item.event_kind),
                reason_code: enum_name(&item.reason_code),
                field_name: item.field_name.clone(),
                field_role: enum_name(&item.field_role),
                expected_coverage_value_units: ranked.expected_coverage_value_units,
                queue_partition: partition_key(ranked),
                occurrence_count: item.occurrence_summary.total_occurrences,
                uncertainty_flags: ranked.uncertainty_flags.clone(),
                next_commands: item_next_commands(
                    &loaded.path,
                    &ranked.event_key,
                    &loaded.artifact.artifact_content_hash,
                ),
            }
        })
        .collect();
    let report = ListReport {
        schema_version: LIST_SCHEMA_VERSION,
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash,
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS,
        filters,
        page,
        items,
    };
    emit_list_report(&report, &args.emit)?;
    Ok(0)
}

fn run_show(args: &InboxShowCli) -> Result<u8, Box<dyn Error>> {
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    let Some(item) = item_by_key(&loaded.artifact, &args.event_key).cloned() else {
        return emit_refusal(
            RefusalCode::EParse,
            "Inbox event key was not found",
            json!({ "event_key": args.event_key, "inbox": path_string(&args.inbox) }),
            Some(format!(
                "canon inbox list --inbox {}",
                shell_path(&args.inbox)
            )),
            &args.emit,
        );
    };
    let ranked_item = ranked_by_key(&loaded.ranking, &args.event_key)
        .expect("ranking includes every finalized inbox item")
        .clone();
    let report = ShowReport {
        schema_version: SHOW_SCHEMA_VERSION,
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash.clone(),
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS,
        event_key: args.event_key.clone(),
        item,
        ranked_item,
        next_commands: item_next_commands(
            &args.inbox,
            &args.event_key,
            &loaded.artifact.artifact_content_hash,
        ),
    };
    emit_show_report(&report, &args.emit)?;
    Ok(0)
}

fn run_explain(args: &InboxExplainCli) -> Result<u8, Box<dyn Error>> {
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    let Some(item) = item_by_key(&loaded.artifact, &args.event_key) else {
        return emit_refusal(
            RefusalCode::EParse,
            "Inbox event key was not found",
            json!({ "event_key": args.event_key, "inbox": path_string(&args.inbox) }),
            Some(format!(
                "canon inbox list --inbox {}",
                shell_path(&args.inbox)
            )),
            &args.emit,
        );
    };
    let ranked_item = ranked_by_key(&loaded.ranking, &args.event_key)
        .expect("ranking includes every finalized inbox item")
        .clone();
    let report = ExplainReport {
        schema_version: EXPLAIN_SCHEMA_VERSION,
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash.clone(),
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS,
        event_key: args.event_key.clone(),
        score: ranked_item,
        provenance: provenance(item),
        next_commands: item_next_commands(
            &args.inbox,
            &args.event_key,
            &loaded.artifact.artifact_content_hash,
        ),
    };
    emit_explain_report(&report, &args.emit)?;
    Ok(0)
}

fn run_stats(args: &InboxStatsCli) -> Result<u8, Box<dyn Error>> {
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    let report = StatsReport {
        schema_version: STATS_SCHEMA_VERSION,
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash.clone(),
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS,
        inbox_summary: loaded.artifact.summary,
        ranking_summary: loaded.ranking.summary,
        next_commands: top_level_next_commands(&args.inbox),
    };
    emit_stats_report(&report, &args.emit)?;
    Ok(0)
}

fn run_export_review(args: &InboxExportReviewCli) -> Result<u8, Box<dyn Error>> {
    let limit = match checked_limit(args.limit, &args.emit) {
        Ok(limit) => limit,
        Err(exit) => return Ok(exit),
    };
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    let filters = InboxFilters {
        event_kind: normalize_filter_values(&args.event_kind),
        reason_code: normalize_filter_values(&args.reason_code),
        field_role: normalize_filter_values(&args.field_role),
        partition: normalize_filter_values(&args.partition),
    };
    let (ranked, page) =
        match page_ranked_items(&loaded, &filters, args.cursor.as_deref(), limit, &args.emit) {
            Ok(page) => page,
            Err(exit) => return Ok(exit),
        };
    let decisions = ranked
        .iter()
        .map(|ranked| ReviewDecision {
            review_id: review_id(&ranked.event_key),
            action: GroupReviewAction::Split,
            member_event_keys: vec![ranked.event_key.clone()],
            operator_ref: "operator:pending".to_string(),
            reason: "pending_operator_review".to_string(),
            reviewed_at: "1970-01-01T00:00:00Z".to_string(),
            identity_status: NO_IDENTITY_STATUS.to_string(),
        })
        .collect::<Vec<_>>();
    let artifact = ReviewExportArtifact {
        schema_version: REVIEW_EXPORT_SCHEMA_VERSION.to_string(),
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash,
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS.to_string(),
        page: ReviewPage {
            limit: page.limit,
            cursor: page.cursor,
            next_cursor: page.next_cursor,
            total_filtered: page.total_filtered,
            returned: page.returned,
        },
        decisions,
    };
    let bytes = serde_json::to_vec(&artifact)?;
    if let Some(out) = &args.out
        && let Err(error) = write_new_file(out, &bytes)
    {
        return emit_refusal(
            RefusalCode::EIo,
            "Could not write inbox review export",
            json!({ "out": path_string(out), "error": error.to_string() }),
            Some("choose a new output path and rerun canon inbox export-review".to_string()),
            &args.emit,
        );
    }
    emit_review_export(&artifact, args.out.as_ref(), &args.emit)?;
    Ok(0)
}

fn run_apply_review(args: &InboxApplyReviewCli) -> Result<u8, Box<dyn Error>> {
    let loaded = match load_inbox_with_policy(&args.inbox, None, &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    if args.expected_inbox_hash != loaded.artifact.artifact_content_hash {
        return emit_refusal(
            RefusalCode::EParse,
            "Inbox review application refused stale inbox hash",
            json!({
                "expected_inbox_hash": args.expected_inbox_hash,
                "actual_inbox_hash": loaded.artifact.artifact_content_hash,
            }),
            Some(format!(
                "canon inbox stats --inbox {}",
                shell_path(&args.inbox)
            )),
            &args.emit,
        );
    }
    let review = match read_json_file::<ReviewExportArtifact>(&args.review) {
        Ok(review) => review,
        Err(error) => {
            return emit_refusal(
                RefusalCode::EParse,
                "Could not read inbox review decisions",
                json!({ "review": path_string(&args.review), "error": error }),
                Some("rerun canon inbox export-review or repair the review JSON".to_string()),
                &args.emit,
            );
        }
    };
    if review.source_inbox_artifact_hash != loaded.artifact.artifact_content_hash {
        return emit_refusal(
            RefusalCode::EParse,
            "Inbox review decisions were produced for a different inbox artifact",
            json!({
                "review_inbox_hash": review.source_inbox_artifact_hash,
                "actual_inbox_hash": loaded.artifact.artifact_content_hash,
            }),
            Some("export a fresh review queue for this inbox hash".to_string()),
            &args.emit,
        );
    }
    let patches = match review_patches(review.decisions) {
        Ok(patches) => patches,
        Err(message) => {
            return emit_refusal(
                RefusalCode::EParse,
                "Inbox review decisions are malformed",
                json!({ "error": message }),
                Some("repair the review decisions and rerun canon inbox apply-review".to_string()),
                &args.emit,
            );
        }
    };
    let plan = UnresolvedGroupingPlan {
        policy_id: "canon.inbox.apply_review.default".to_string(),
        grouping_surface_roles: Vec::new(),
        protected_surface_roles: Vec::new(),
        cannot_group: Vec::new(),
        review_patches: patches,
    };
    let groups = match group_unresolved_artifact(&loaded.artifact, plan) {
        Ok(groups) => groups,
        Err(error) => {
            return emit_inbox_error(
                "Inbox review decisions could not be applied",
                error,
                Some(
                    "inspect review patch member_event_keys and protected grouping boundaries"
                        .to_string(),
                ),
                &args.emit,
            );
        }
    };
    let bytes = match super::group::canonical_group_json_bytes(&groups) {
        Ok(bytes) => bytes,
        Err(error) => {
            return emit_inbox_error(
                "Grouped unresolved artifact could not be serialized",
                error,
                None,
                &args.emit,
            );
        }
    };
    if let Err(error) = write_new_file(&args.out, &bytes) {
        return emit_refusal(
            RefusalCode::EIo,
            "Could not write grouped inbox artifact",
            json!({ "out": path_string(&args.out), "error": error.to_string() }),
            Some("choose a new output path and rerun canon inbox apply-review".to_string()),
            &args.emit,
        );
    }
    let receipt = ReviewApplyReceipt {
        schema_version: REVIEW_APPLY_SCHEMA_VERSION,
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash,
        review_path: path_string(&args.review),
        output_path: path_string(&args.out),
        identity_status: NO_IDENTITY_STATUS,
        applied_decision_count: groups.plan.review_patches.len(),
        grouped_artifact_hash: groups.artifact_content_hash,
        next_commands: BTreeMap::from([(
            "plan_entity".to_string(),
            format!(
                "canon inbox plan-entity --inbox {} --expected-inbox-hash <HASH> --out <REQUEST.json>",
                shell_path(&args.inbox)
            ),
        )]),
    };
    emit_apply_receipt(&receipt, &args.emit)?;
    Ok(0)
}

fn run_plan_entity(args: &InboxPlanEntityCli) -> Result<u8, Box<dyn Error>> {
    let limit = match checked_limit(args.limit, &args.emit) {
        Ok(limit) => limit,
        Err(exit) => return Ok(exit),
    };
    let loaded = match load_inbox_with_policy(&args.inbox, args.policy.as_ref(), &args.emit) {
        Ok(loaded) => loaded,
        Err(exit) => return Ok(exit),
    };
    if args.expected_inbox_hash != loaded.artifact.artifact_content_hash {
        return emit_refusal(
            RefusalCode::EParse,
            "Inbox entity planning refused stale inbox hash",
            json!({
                "expected_inbox_hash": args.expected_inbox_hash,
                "actual_inbox_hash": loaded.artifact.artifact_content_hash,
            }),
            Some(format!(
                "canon inbox stats --inbox {}",
                shell_path(&args.inbox)
            )),
            &args.emit,
        );
    }
    let selected_ranked = if args.event_key.is_empty() {
        loaded
            .ranking
            .ranked_items
            .iter()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for key in &args.event_key {
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(ranked) = ranked_by_key(&loaded.ranking, key) else {
                return emit_refusal(
                    RefusalCode::EParse,
                    "Inbox entity planning event key was not found",
                    json!({ "event_key": key }),
                    Some(format!(
                        "canon inbox list --inbox {}",
                        shell_path(&args.inbox)
                    )),
                    &args.emit,
                );
            };
            selected.push(ranked.clone());
        }
        selected
    };
    if selected_ranked.is_empty() {
        return emit_refusal(
            RefusalCode::EEmptyInput,
            "Inbox entity planning found no selected items",
            json!({ "inbox": path_string(&args.inbox) }),
            Some(format!(
                "canon inbox list --inbox {}",
                shell_path(&args.inbox)
            )),
            &args.emit,
        );
    }
    let selected_items = selected_ranked
        .iter()
        .map(|ranked| {
            let item = item_by_key(&loaded.artifact, &ranked.event_key)
                .expect("ranked items always reference inbox items");
            EntityPlanItem {
                event_key: ranked.event_key.clone(),
                rank: ranked.rank,
                field_name: item.field_name.clone(),
                field_role: enum_name(&item.field_role),
                reason_code: enum_name(&item.reason_code),
                occurrence_refs: item.occurrences.clone(),
            }
        })
        .collect::<Vec<_>>();
    let mode = match args.mode {
        InboxEntityPlanMode::Cluster => "cluster",
        InboxEntityPlanMode::Link => "link",
    };
    let candidate_event_keys = selected_items
        .iter()
        .map(|item| item.event_key.clone())
        .collect::<Vec<_>>();
    let plan = EntityPlanArtifact {
        schema_version: ENTITY_PLAN_SCHEMA_VERSION.to_string(),
        source_inbox_artifact_hash: loaded.artifact.artifact_content_hash.clone(),
        ranking_artifact_hash: loaded.ranking.artifact_content_hash,
        identity_status: NO_IDENTITY_STATUS.to_string(),
        mode: mode.to_string(),
        bounded_selection: EntityPlanBounds {
            max_items: limit,
            selected_count: selected_items.len(),
            selection_rule: if args.event_key.is_empty() {
                "top_ranked"
            } else {
                "explicit_event_keys"
            }
            .to_string(),
        },
        selected_items,
        request: EntityWorkbenchRequest {
            command_family: "canon entity",
            requested_mode: mode.to_string(),
            identity_decision: NO_IDENTITY_STATUS,
            required_inputs: vec!["rows", "profile_or_strategy", "registry", "work_dir"],
            candidate_event_keys,
            preview_command: match args.mode {
                InboxEntityPlanMode::Cluster => {
                    "canon entity run <ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY> --work-dir <DIR> --emit json".to_string()
                }
                InboxEntityPlanMode::Link => {
                    "canon entity link <REFERENCE_ROWS> <TARGET_ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY> --work-dir <DIR> --emit json".to_string()
                }
            },
        },
        next_commands: BTreeMap::from([(
            "run_entity".to_string(),
            "Fill required_inputs in the request, then run the preview_command explicitly"
                .to_string(),
        )]),
    };
    let bytes = serde_json::to_vec(&plan)?;
    if let Err(error) = write_new_file(&args.out, &bytes) {
        return emit_refusal(
            RefusalCode::EIo,
            "Could not write inbox entity plan",
            json!({ "out": path_string(&args.out), "error": error.to_string() }),
            Some("choose a new output path and rerun canon inbox plan-entity".to_string()),
            &args.emit,
        );
    }
    emit_entity_plan(&plan, &args.out, &args.emit)?;
    Ok(0)
}

fn load_inbox_with_policy(
    inbox_path: &Path,
    policy_path: Option<&PathBuf>,
    emit: &RegistryEmitMode,
) -> Result<LoadedInbox, u8> {
    let raw = match read_json_file::<UnresolvedInboxArtifact>(inbox_path) {
        Ok(raw) => raw,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EParse,
                "Could not read unresolved inbox artifact",
                json!({ "inbox": path_string(inbox_path), "error": error }),
                Some("provide a canon.unresolved.inbox.v1 JSON artifact".to_string()),
                emit,
            )
            .unwrap_or(2));
        }
    };
    let artifact = match finalize_artifact(raw) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Err(emit_inbox_error(
                "Unresolved inbox artifact failed validation",
                error,
                Some("repair the inbox artifact and rerun canon inbox".to_string()),
                emit,
            )
            .unwrap_or(2));
        }
    };
    let policy = match load_policy(policy_path, &artifact) {
        Ok(policy) => policy,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EParse,
                "Could not read inbox priority policy",
                json!({ "policy": policy_path.map(|path| path_string(path)), "error": error }),
                Some("provide a canon.inbox.priority_policy.v1 JSON policy".to_string()),
                emit,
            )
            .unwrap_or(2));
        }
    };
    let ranking = match rank_inbox(&artifact, policy) {
        Ok(ranking) => ranking,
        Err(error) => {
            return Err(emit_inbox_error(
                "Inbox priority ranking failed",
                error,
                Some("repair the inbox priority policy or inbox artifact".to_string()),
                emit,
            )
            .unwrap_or(2));
        }
    };
    Ok(LoadedInbox {
        path: inbox_path.to_path_buf(),
        artifact,
        ranking,
    })
}

fn load_policy(
    policy_path: Option<&PathBuf>,
    artifact: &UnresolvedInboxArtifact,
) -> Result<PriorityPolicy, String> {
    match policy_path {
        Some(path) => read_json_file(path),
        None => {
            let as_of = artifact
                .items
                .iter()
                .map(|item| item.last_seen_at.as_str())
                .max()
                .unwrap_or("1970-01-01T00:00:00Z");
            Ok(PriorityPolicy::baseline(
                "canon.inbox.priority.default",
                "1",
                as_of,
            ))
        }
    }
}

fn page_ranked_items(
    loaded: &LoadedInbox,
    filters: &InboxFilters,
    cursor: Option<&str>,
    limit: usize,
    emit: &RegistryEmitMode,
) -> Result<(Vec<RankedInboxItem>, PageInfo), u8> {
    let offset = parse_cursor(cursor, &loaded.artifact.artifact_content_hash, emit)?.unwrap_or(0);
    let filtered = loaded
        .ranking
        .ranked_items
        .iter()
        .filter(|ranked| {
            item_by_key(&loaded.artifact, &ranked.event_key)
                .map(|item| item_matches(item, ranked, filters))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    let total_filtered = filtered.len();
    let page_items = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let end = offset.saturating_add(page_items.len());
    let next_cursor = if end < total_filtered {
        Some(format!(
            "offset:{end}:{}",
            loaded.artifact.artifact_content_hash
        ))
    } else {
        None
    };
    Ok((
        page_items,
        PageInfo {
            limit,
            cursor: cursor.map(str::to_string),
            next_cursor,
            total_filtered,
            returned: end.saturating_sub(offset),
        },
    ))
}

fn parse_cursor(
    cursor: Option<&str>,
    expected_hash: &str,
    emit: &RegistryEmitMode,
) -> Result<Option<usize>, u8> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "offset" || parts[2] != "blake3" {
        return Err(emit_refusal(
            RefusalCode::EParse,
            "Inbox cursor is malformed",
            json!({ "cursor": cursor }),
            Some("rerun canon inbox list without --cursor".to_string()),
            emit,
        )
        .unwrap_or(2));
    }
    let offset = match parts[1].parse::<usize>() {
        Ok(offset) => offset,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EParse,
                "Inbox cursor offset is malformed",
                json!({ "cursor": cursor, "error": error.to_string() }),
                Some("rerun canon inbox list without --cursor".to_string()),
                emit,
            )
            .unwrap_or(2));
        }
    };
    let cursor_hash = format!("blake3:{}", parts[3]);
    if cursor_hash != expected_hash {
        return Err(emit_refusal(
            RefusalCode::EParse,
            "Inbox cursor is stale for this inbox artifact",
            json!({ "cursor_hash": cursor_hash, "actual_inbox_hash": expected_hash }),
            Some("rerun canon inbox list without --cursor".to_string()),
            emit,
        )
        .unwrap_or(2));
    }
    Ok(Some(offset))
}

fn item_matches(
    item: &UnresolvedInboxItem,
    ranked: &RankedInboxItem,
    filters: &InboxFilters,
) -> bool {
    matches_filter(&enum_name(&item.event_kind), &filters.event_kind)
        && matches_filter(&enum_name(&item.reason_code), &filters.reason_code)
        && matches_filter(&enum_name(&item.field_role), &filters.field_role)
        && matches_filter(&partition_key(ranked), &filters.partition)
}

fn matches_filter(value: &str, filters: &[String]) -> bool {
    filters.is_empty() || filters.iter().any(|filter| filter == value)
}

fn item_by_key<'a>(
    artifact: &'a UnresolvedInboxArtifact,
    event_key: &str,
) -> Option<&'a UnresolvedInboxItem> {
    artifact
        .items
        .iter()
        .find(|item| item.event_key == event_key)
}

fn ranked_by_key<'a>(
    ranking: &'a InboxPriorityRankingArtifact,
    event_key: &str,
) -> Option<&'a RankedInboxItem> {
    ranking
        .ranked_items
        .iter()
        .find(|item| item.event_key == event_key)
}

fn provenance(item: &UnresolvedInboxItem) -> InboxProvenance {
    InboxProvenance {
        field_name: item.field_name.clone(),
        field_role: enum_name(&item.field_role),
        event_kind: enum_name(&item.event_kind),
        reason_code: enum_name(&item.reason_code),
        occurrence_count: item.occurrence_summary.total_occurrences,
        first_seen_at: item.first_seen_at.clone(),
        last_seen_at: item.last_seen_at.clone(),
        source_refs: item
            .occurrences
            .iter()
            .map(|occurrence| occurrence.source_ref.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        namespace_hints: item
            .namespace_hints
            .iter()
            .map(|hint| format!("{}:{}", hint.namespace, hint.source))
            .collect(),
    }
}

fn review_patches(decisions: Vec<ReviewDecision>) -> Result<Vec<GroupReviewPatch>, String> {
    let mut patches = Vec::new();
    let mut review_ids = BTreeSet::new();
    for decision in decisions {
        if !review_ids.insert(decision.review_id.clone()) {
            return Err(format!("duplicate review_id {}", decision.review_id));
        }
        if decision.member_event_keys.is_empty() {
            return Err(format!(
                "review_id {} must include member_event_keys",
                decision.review_id
            ));
        }
        if decision.identity_status != NO_IDENTITY_STATUS {
            return Err(format!(
                "review_id {} must preserve identity_status={NO_IDENTITY_STATUS}",
                decision.review_id
            ));
        }
        patches.push(GroupReviewPatch {
            patch_id: decision.review_id,
            action: decision.action,
            member_event_keys: decision.member_event_keys,
            operator_ref: decision.operator_ref,
            reason: decision.reason,
            reviewed_at: decision.reviewed_at,
        });
    }
    Ok(patches)
}

fn checked_limit(limit: usize, emit: &RegistryEmitMode) -> Result<usize, u8> {
    if limit == 0 {
        return Err(emit_refusal(
            RefusalCode::EParse,
            "Inbox limit must be greater than zero",
            json!({ "limit": limit }),
            Some("rerun with --limit 1 or greater".to_string()),
            emit,
        )
        .unwrap_or(2));
    }
    Ok(limit)
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)
}

fn emit_list_report(report: &ListReport, emit: &RegistryEmitMode) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        RegistryEmitMode::Summary => println!(
            "inbox items={} returned={} next_cursor={} top={}",
            report.page.total_filtered,
            report.page.returned,
            report.page.next_cursor.as_deref().unwrap_or("none"),
            report
                .items
                .first()
                .map(|item| item.event_key.as_str())
                .unwrap_or("none")
        ),
    }
    Ok(())
}

fn emit_show_report(report: &ShowReport, emit: &RegistryEmitMode) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        RegistryEmitMode::Summary => println!(
            "inbox item={} rank={} reason={} occurrences={} next=[{}]",
            report.event_key,
            report.ranked_item.rank,
            enum_name(&report.item.reason_code),
            report.item.occurrence_summary.total_occurrences,
            next_command_keys(&report.next_commands)
        ),
    }
    Ok(())
}

fn emit_explain_report(
    report: &ExplainReport,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        RegistryEmitMode::Summary => println!(
            "inbox explain={} rank={} score={} components={} identity_status={}",
            report.event_key,
            report.score.rank,
            report.score.expected_coverage_value_units,
            report.score.components.len(),
            report.identity_status
        ),
    }
    Ok(())
}

fn emit_stats_report(report: &StatsReport, emit: &RegistryEmitMode) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        RegistryEmitMode::Summary => println!(
            "inbox hash={} items={} occurrences={} ranked={} top_score={}",
            report.source_inbox_artifact_hash,
            report.inbox_summary.total_items,
            report.inbox_summary.total_occurrences,
            report.ranking_summary.total_items,
            report.ranking_summary.highest_expected_coverage_value_units
        ),
    }
    Ok(())
}

fn emit_review_export(
    artifact: &ReviewExportArtifact,
    out: Option<&PathBuf>,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(artifact)?),
        RegistryEmitMode::Summary => println!(
            "inbox review_export decisions={} out={} next_cursor={}",
            artifact.decisions.len(),
            out.map(|path| path_string(path))
                .unwrap_or_else(|| "stdout".to_string()),
            artifact.page.next_cursor.as_deref().unwrap_or("none")
        ),
    }
    Ok(())
}

fn emit_apply_receipt(
    receipt: &ReviewApplyReceipt,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(receipt)?),
        RegistryEmitMode::Summary => println!(
            "inbox apply_review decisions={} out={} groups_hash={} identity_status={}",
            receipt.applied_decision_count,
            receipt.output_path,
            receipt.grouped_artifact_hash,
            receipt.identity_status
        ),
    }
    Ok(())
}

fn emit_entity_plan(
    plan: &EntityPlanArtifact,
    out: &Path,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(plan)?),
        RegistryEmitMode::Summary => println!(
            "inbox entity_plan mode={} selected={} out={} identity_status={}",
            plan.mode,
            plan.bounded_selection.selected_count,
            path_string(out),
            plan.identity_status
        ),
    }
    Ok(())
}

fn emit_inbox_error(
    message: &str,
    error: InboxError,
    next_command: Option<String>,
    emit: &RegistryEmitMode,
) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        message,
        json!({ "inbox_error_code": enum_name(&error.code), "message": error.message }),
        next_command,
        emit,
    )
}

fn emit_refusal(
    code: RefusalCode,
    message: impl Into<String>,
    detail: Value,
    next_command: Option<String>,
    emit: &RegistryEmitMode,
) -> Result<u8, Box<dyn Error>> {
    let output = refusal::create_refusal(code, message.into(), detail, next_command);
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
        RegistryEmitMode::Summary => eprintln!("{}", refusal_summary(&output)),
    }
    Ok(2)
}

fn refusal_summary(output: &CanonOutput) -> String {
    let Some(refusal) = &output.refusal else {
        return "refused code=unknown message=\"unknown refusal\"".to_string();
    };
    format!(
        "refused code={} message=\"{}\" next=\"{}\"",
        serde_json::to_string(&refusal.code).unwrap_or_else(|_| "\"E_PARSE\"".to_string()),
        refusal.message,
        refusal.next_command.as_deref().unwrap_or("")
    )
}

fn item_next_commands(
    inbox_path: &Path,
    event_key: &str,
    inbox_hash: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "show".to_string(),
            format!(
                "canon inbox show --inbox {} --event-key {}",
                shell_path(inbox_path),
                event_key
            ),
        ),
        (
            "explain".to_string(),
            format!(
                "canon inbox explain --inbox {} --event-key {}",
                shell_path(inbox_path),
                event_key
            ),
        ),
        (
            "plan_entity".to_string(),
            format!(
                "canon inbox plan-entity --inbox {} --event-key {} --expected-inbox-hash {} --out <REQUEST.json>",
                shell_path(inbox_path),
                event_key,
                inbox_hash
            ),
        ),
    ])
}

fn top_level_next_commands(inbox_path: &Path) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "list".to_string(),
            format!("canon inbox list --inbox {}", shell_path(inbox_path)),
        ),
        (
            "export_review".to_string(),
            format!(
                "canon inbox export-review --inbox {} --out <REVIEW.json>",
                shell_path(inbox_path)
            ),
        ),
    ])
}

fn enum_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn partition_key(ranked: &RankedInboxItem) -> String {
    format!(
        "profile={}|registry={}|role={}|source={}|privacy={}",
        ranked.queue_partition.profile,
        ranked.queue_partition.registry,
        ranked.queue_partition.role,
        ranked.queue_partition.source,
        ranked.queue_partition.privacy_class
    )
}

fn normalize_filter_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn review_id(event_key: &str) -> String {
    hash_bytes(format!("{REVIEW_EXPORT_SCHEMA_VERSION}:{event_key}").as_bytes())
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn next_command_keys(commands: &BTreeMap<String, String>) -> String {
    commands.keys().cloned().collect::<Vec<_>>().join(",")
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn shell_path(path: &Path) -> String {
    let value = path_string(path);
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
