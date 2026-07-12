#![forbid(unsafe_code)]

//! Deterministic grouping for recurring unresolved inbox surfaces.
//!
//! Groups are review/work-queue compression only. They intentionally carry no
//! canonical ID, no candidate winner, and no identity assertion.

use crate::inbox::{
    InboxError, InboxErrorCode, InboxEventKind, InboxFieldRole, InboxOccurrenceRef,
    InboxReasonCode, InboxResult, NamespaceHint, NormalizedSurfaceFingerprint, OccurrenceSummary,
    ProfileFieldRef, TemporalScope, UnresolvedInboxArtifact, UnresolvedInboxItem,
    finalize_artifact,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_UNRESOLVED_GROUPS_VERSION: &str = "canon.unresolved.groups.v1";
pub const GROUP_IDENTITY_STATUS: &str = "no_identity_assertion";
const REPRESENTATIVE_SELECTION: &str = "stable_order:first_seen_at,event_key,field_name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnresolvedGroupingPlan {
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grouping_surface_roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_surface_roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cannot_group: Vec<CannotGroupRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_patches: Vec<GroupReviewPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannotGroupRule {
    pub rule_id: String,
    pub left_event_key: String,
    pub right_event_key: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupReviewAction {
    Split,
    Merge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupReviewPatch {
    pub patch_id: String,
    pub action: GroupReviewAction,
    pub member_event_keys: Vec<String>,
    pub operator_ref: String,
    pub reason: String,
    pub reviewed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnresolvedGroupsSummary {
    #[serde(default)]
    pub total_groups: u64,
    #[serde(default)]
    pub total_members: u64,
    #[serde(default)]
    pub total_occurrences: u64,
    #[serde(default)]
    pub reviewed_patch_count: u64,
    #[serde(default)]
    pub by_reason_code: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_field_role: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnresolvedGroupsArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub identity_status: String,
    pub source_inbox_artifact_hash: String,
    pub plan: UnresolvedGroupingPlan,
    pub summary: UnresolvedGroupsSummary,
    #[serde(default)]
    pub groups: Vec<UnresolvedGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedGroup {
    pub group_id: String,
    pub group_key_hashes: Vec<String>,
    pub hard_boundary_hash: String,
    pub representative_event_key: String,
    pub representative_selection: String,
    pub grouping_keys: Vec<UnresolvedGroupKey>,
    pub member_count: u64,
    pub occurrence_summary: OccurrenceSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_patch_ids: Vec<String>,
    pub members: Vec<UnresolvedGroupMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedGroupKey {
    pub event_kind: InboxEventKind,
    pub reason_code: InboxReasonCode,
    pub field_name: String,
    pub field_role: InboxFieldRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<ProfileFieldRef>,
    #[serde(default)]
    pub grouping_surface_fingerprints: Vec<NormalizedSurfaceFingerprint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_surface_fingerprints: Vec<NormalizedSurfaceFingerprint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespace_hints: Vec<NamespaceHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_scope: Option<TemporalScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedGroupMember {
    pub event_key: String,
    pub event_kind: InboxEventKind,
    pub reason_code: InboxReasonCode,
    pub field_name: String,
    pub field_role: InboxFieldRole,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub occurrence_summary: OccurrenceSummary,
    pub occurrences: Vec<InboxOccurrenceRef>,
}

#[derive(Debug, Clone)]
struct KeyedItem {
    item: UnresolvedInboxItem,
    group_key: UnresolvedGroupKey,
    group_key_hash: String,
    hard_boundary_hash: String,
}

#[derive(Debug, Clone)]
struct MutableGroup {
    grouping_keys: Vec<UnresolvedGroupKey>,
    group_key_hashes: Vec<String>,
    hard_boundary_hash: String,
    members: Vec<KeyedItem>,
    review_patch_ids: BTreeSet<String>,
}

#[derive(Serialize)]
struct HardBoundaryKey<'a> {
    event_kind: &'a InboxEventKind,
    reason_code: &'a InboxReasonCode,
    field_name: &'a str,
    field_role: &'a InboxFieldRole,
    profile_ref: &'a Option<ProfileFieldRef>,
    protected_surface_fingerprints: &'a [NormalizedSurfaceFingerprint],
    namespace_hints: &'a [NamespaceHint],
    temporal_scope: &'a Option<TemporalScope>,
}

pub fn group_unresolved_artifact(
    inbox: &UnresolvedInboxArtifact,
    plan: UnresolvedGroupingPlan,
) -> InboxResult<UnresolvedGroupsArtifact> {
    let inbox = finalize_artifact(inbox.clone())?;
    let plan = normalize_plan(plan)?;
    let cannot_pairs = cannot_pairs(&plan.cannot_group);
    let mut groups = build_auto_groups(&inbox.items, &plan, &cannot_pairs)?;
    apply_review_patches(&mut groups, &plan.review_patches, &cannot_pairs)?;

    let mut groups = groups
        .into_iter()
        .map(finalize_group)
        .collect::<InboxResult<Vec<_>>>()?;
    groups.sort_by(group_cmp);

    let mut artifact = UnresolvedGroupsArtifact {
        version: CANON_UNRESOLVED_GROUPS_VERSION.to_string(),
        artifact_content_hash: String::new(),
        identity_status: GROUP_IDENTITY_STATUS.to_string(),
        source_inbox_artifact_hash: inbox.artifact_content_hash,
        plan,
        summary: UnresolvedGroupsSummary::default(),
        groups,
    };
    artifact.summary = build_summary(&artifact);
    artifact.artifact_content_hash = hash_without_self(&artifact)?;
    Ok(artifact)
}

pub fn canonical_group_json_bytes(artifact: &UnresolvedGroupsArtifact) -> InboxResult<Vec<u8>> {
    serde_json::to_vec(artifact).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize unresolved groups artifact: {error}"
        ))
    })
}

fn normalize_plan(mut plan: UnresolvedGroupingPlan) -> InboxResult<UnresolvedGroupingPlan> {
    plan.policy_id = plan.policy_id.trim().to_string();
    if plan.policy_id.is_empty() {
        return Err(artifact_contract_error(
            "grouping plan requires a non-empty policy_id",
        ));
    }

    normalize_string_list(&mut plan.grouping_surface_roles);
    normalize_string_list(&mut plan.protected_surface_roles);

    for rule in &mut plan.cannot_group {
        rule.rule_id = rule.rule_id.trim().to_string();
        rule.left_event_key = normalized_hash(&rule.left_event_key, "cannot_group.left_event_key")?;
        rule.right_event_key =
            normalized_hash(&rule.right_event_key, "cannot_group.right_event_key")?;
        rule.reason = rule.reason.trim().to_string();
        if rule.rule_id.is_empty() || rule.reason.is_empty() {
            return Err(artifact_contract_error(
                "cannot_group rules require non-empty rule_id and reason",
            ));
        }
        if rule.left_event_key == rule.right_event_key {
            return Err(artifact_contract_error(
                "cannot_group rules must reference two distinct event keys",
            ));
        }
        if rule.left_event_key > rule.right_event_key {
            std::mem::swap(&mut rule.left_event_key, &mut rule.right_event_key);
        }
    }
    plan.cannot_group.sort_by(cannot_rule_cmp);
    plan.cannot_group.dedup_by(|left, right| {
        left.rule_id == right.rule_id
            && left.left_event_key == right.left_event_key
            && left.right_event_key == right.right_event_key
            && left.reason == right.reason
    });

    for patch in &mut plan.review_patches {
        patch.patch_id = patch.patch_id.trim().to_string();
        patch.operator_ref = patch.operator_ref.trim().to_string();
        patch.reason = patch.reason.trim().to_string();
        patch.reviewed_at = canonical_timestamp(&patch.reviewed_at, "review_patch.reviewed_at")?;
        normalize_string_list(&mut patch.member_event_keys);
        for event_key in &mut patch.member_event_keys {
            *event_key = normalized_hash(event_key, "review_patch.member_event_keys")?;
        }
        if patch.patch_id.is_empty() || patch.operator_ref.is_empty() || patch.reason.is_empty() {
            return Err(artifact_contract_error(
                "review patches require non-empty patch_id, operator_ref, and reason",
            ));
        }
        let min_members = match patch.action {
            GroupReviewAction::Split => 1,
            GroupReviewAction::Merge => 2,
        };
        if patch.member_event_keys.len() < min_members {
            return Err(artifact_contract_error(format!(
                "{:?} review patch requires at least {min_members} member event key(s)",
                patch.action
            )));
        }
    }
    plan.review_patches.sort_by(review_patch_cmp);
    Ok(plan)
}

fn build_auto_groups(
    items: &[UnresolvedInboxItem],
    plan: &UnresolvedGroupingPlan,
    cannot_pairs: &BTreeSet<(String, String)>,
) -> InboxResult<Vec<MutableGroup>> {
    let mut buckets: BTreeMap<String, Vec<KeyedItem>> = BTreeMap::new();

    for item in items {
        let keyed = keyed_item(item.clone(), plan)?;
        let bucket_key = serde_json::to_string(&keyed.group_key).map_err(|error| {
            artifact_contract_error(format!("failed to serialize unresolved group key: {error}"))
        })?;
        buckets.entry(bucket_key).or_default().push(keyed);
    }

    let mut groups = Vec::new();
    for mut bucket_items in buckets.into_values() {
        bucket_items.sort_by(keyed_item_cmp);
        let mut partitions: Vec<Vec<KeyedItem>> = Vec::new();

        for item in bucket_items {
            if let Some(partition) = partitions.iter_mut().find(|partition| {
                partition.iter().all(|existing| {
                    !cannot_pairs
                        .contains(&event_pair(&existing.item.event_key, &item.item.event_key))
                })
            }) {
                partition.push(item);
            } else {
                partitions.push(vec![item]);
            }
        }

        for partition in partitions {
            let Some(first) = partition.first() else {
                continue;
            };
            groups.push(MutableGroup {
                grouping_keys: vec![first.group_key.clone()],
                group_key_hashes: vec![first.group_key_hash.clone()],
                hard_boundary_hash: first.hard_boundary_hash.clone(),
                members: partition,
                review_patch_ids: BTreeSet::new(),
            });
        }
    }

    Ok(groups)
}

fn keyed_item(item: UnresolvedInboxItem, plan: &UnresolvedGroupingPlan) -> InboxResult<KeyedItem> {
    let grouping_roles: BTreeSet<&str> = plan
        .grouping_surface_roles
        .iter()
        .map(String::as_str)
        .collect();
    let protected_roles: BTreeSet<&str> = plan
        .protected_surface_roles
        .iter()
        .map(String::as_str)
        .collect();

    let mut grouping_surface_fingerprints = Vec::new();
    let mut protected_surface_fingerprints = Vec::new();
    for fingerprint in &item.surface_fingerprints {
        let is_protected = protected_roles.contains(fingerprint.surface_role.as_str());
        let is_grouping = if grouping_roles.is_empty() {
            !is_protected
        } else {
            grouping_roles.contains(fingerprint.surface_role.as_str())
        };
        if is_grouping {
            grouping_surface_fingerprints.push(fingerprint.clone());
        }
        if is_protected {
            protected_surface_fingerprints.push(fingerprint.clone());
        }
    }
    if grouping_surface_fingerprints.is_empty() && protected_surface_fingerprints.is_empty() {
        return Err(artifact_contract_error(
            "grouping policy selected no surface fingerprints for an inbox item",
        ));
    }
    grouping_surface_fingerprints.sort_by(fingerprint_cmp);
    grouping_surface_fingerprints.dedup();
    protected_surface_fingerprints.sort_by(fingerprint_cmp);
    protected_surface_fingerprints.dedup();

    let group_key = UnresolvedGroupKey {
        event_kind: item.event_kind,
        reason_code: item.reason_code,
        field_name: item.field_name.clone(),
        field_role: item.field_role,
        profile_ref: item.profile_ref.clone(),
        grouping_surface_fingerprints,
        protected_surface_fingerprints,
        namespace_hints: item.namespace_hints.clone(),
        temporal_scope: item.temporal_scope.clone(),
    };
    let group_key_hash = hash_serialize(&group_key)?;
    let hard_boundary_hash = hash_serialize(&HardBoundaryKey {
        event_kind: &group_key.event_kind,
        reason_code: &group_key.reason_code,
        field_name: &group_key.field_name,
        field_role: &group_key.field_role,
        profile_ref: &group_key.profile_ref,
        protected_surface_fingerprints: &group_key.protected_surface_fingerprints,
        namespace_hints: &group_key.namespace_hints,
        temporal_scope: &group_key.temporal_scope,
    })?;

    Ok(KeyedItem {
        item,
        group_key,
        group_key_hash,
        hard_boundary_hash,
    })
}

fn apply_review_patches(
    groups: &mut Vec<MutableGroup>,
    patches: &[GroupReviewPatch],
    cannot_pairs: &BTreeSet<(String, String)>,
) -> InboxResult<()> {
    for patch in patches {
        match patch.action {
            GroupReviewAction::Split => apply_split_patch(groups, patch)?,
            GroupReviewAction::Merge => apply_merge_patch(groups, patch, cannot_pairs)?,
        }
        groups.retain(|group| !group.members.is_empty());
    }
    Ok(())
}

fn apply_split_patch(groups: &mut Vec<MutableGroup>, patch: &GroupReviewPatch) -> InboxResult<()> {
    let keys: BTreeSet<String> = patch.member_event_keys.iter().cloned().collect();
    let origin_indexes = matching_group_indexes(groups, &keys);
    if origin_indexes.is_empty() {
        return Err(artifact_contract_error(format!(
            "split patch {} references no current group members",
            patch.patch_id
        )));
    }
    if origin_indexes.len() > 1 {
        return Err(artifact_contract_error(format!(
            "split patch {} references members from multiple groups",
            patch.patch_id
        )));
    }

    let origin_index = origin_indexes.iter().next().copied().ok_or_else(|| {
        artifact_contract_error(format!(
            "split patch {} references no current group members",
            patch.patch_id
        ))
    })?;
    let selected = take_members(groups, &keys)?;
    let origin = &groups[origin_index];
    groups.push(MutableGroup {
        grouping_keys: unique_group_keys(selected.iter()),
        group_key_hashes: unique_key_hashes(selected.iter()),
        hard_boundary_hash: origin.hard_boundary_hash.clone(),
        members: selected,
        review_patch_ids: BTreeSet::from([patch.patch_id.clone()]),
    });
    if let Some(group) = groups.get_mut(origin_index) {
        group.review_patch_ids.insert(patch.patch_id.clone());
    }
    Ok(())
}

fn apply_merge_patch(
    groups: &mut Vec<MutableGroup>,
    patch: &GroupReviewPatch,
    cannot_pairs: &BTreeSet<(String, String)>,
) -> InboxResult<()> {
    let keys: BTreeSet<String> = patch.member_event_keys.iter().cloned().collect();
    for left in &patch.member_event_keys {
        for right in &patch.member_event_keys {
            if left < right && cannot_pairs.contains(&event_pair(left, right)) {
                return Err(artifact_contract_error(format!(
                    "merge patch {} violates an explicit cannot_group rule",
                    patch.patch_id
                )));
            }
        }
    }

    let mut selected = take_members(groups, &keys)?;
    selected.sort_by(keyed_item_cmp);
    let Some(first) = selected.first() else {
        return Err(artifact_contract_error(format!(
            "merge patch {} references no current group members",
            patch.patch_id
        )));
    };
    if selected
        .iter()
        .any(|item| item.hard_boundary_hash != first.hard_boundary_hash)
    {
        return Err(artifact_contract_error(format!(
            "merge patch {} crosses a protected/namespace/temporal boundary",
            patch.patch_id
        )));
    }

    groups.push(MutableGroup {
        grouping_keys: unique_group_keys(selected.iter()),
        group_key_hashes: unique_key_hashes(selected.iter()),
        hard_boundary_hash: first.hard_boundary_hash.clone(),
        members: selected,
        review_patch_ids: BTreeSet::from([patch.patch_id.clone()]),
    });
    Ok(())
}

fn matching_group_indexes(groups: &[MutableGroup], keys: &BTreeSet<String>) -> BTreeSet<usize> {
    let mut indexes = BTreeSet::new();
    for (index, group) in groups.iter().enumerate() {
        if group
            .members
            .iter()
            .any(|member| keys.contains(&member.item.event_key))
        {
            indexes.insert(index);
        }
    }
    indexes
}

fn take_members(
    groups: &mut [MutableGroup],
    keys: &BTreeSet<String>,
) -> InboxResult<Vec<KeyedItem>> {
    let mut selected = Vec::new();
    for group in groups {
        let mut retained = Vec::new();
        for member in group.members.drain(..) {
            if keys.contains(&member.item.event_key) {
                selected.push(member);
            } else {
                retained.push(member);
            }
        }
        group.members = retained;
    }
    let selected_keys: BTreeSet<String> = selected
        .iter()
        .map(|member| member.item.event_key.clone())
        .collect();
    if selected_keys != *keys {
        let missing = keys
            .difference(&selected_keys)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(artifact_contract_error(format!(
            "review patch references unknown member event key(s): {missing}"
        )));
    }
    Ok(selected)
}

fn finalize_group(mut group: MutableGroup) -> InboxResult<UnresolvedGroup> {
    group.members.sort_by(keyed_item_cmp);
    group.grouping_keys = unique_group_keys(group.members.iter());
    group.group_key_hashes = unique_key_hashes(group.members.iter());

    let representative = group
        .members
        .first()
        .ok_or_else(|| artifact_contract_error("cannot finalize an empty unresolved group"))?;
    let members = group
        .members
        .iter()
        .map(|member| UnresolvedGroupMember {
            event_key: member.item.event_key.clone(),
            event_kind: member.item.event_kind,
            reason_code: member.item.reason_code,
            field_name: member.item.field_name.clone(),
            field_role: member.item.field_role,
            first_seen_at: member.item.first_seen_at.clone(),
            last_seen_at: member.item.last_seen_at.clone(),
            occurrence_summary: member.item.occurrence_summary.clone(),
            occurrences: member.item.occurrences.clone(),
        })
        .collect::<Vec<_>>();
    let occurrence_summary = group_occurrence_summary(&members);
    let review_patch_ids = group.review_patch_ids.into_iter().collect::<Vec<_>>();
    let group_id = hash_serialize(&GroupIdMaterial {
        version: CANON_UNRESOLVED_GROUPS_VERSION,
        group_key_hashes: &group.group_key_hashes,
        member_event_keys: &members
            .iter()
            .map(|member| member.event_key.as_str())
            .collect::<Vec<_>>(),
        review_patch_ids: &review_patch_ids,
    })?;

    Ok(UnresolvedGroup {
        group_id,
        group_key_hashes: group.group_key_hashes,
        hard_boundary_hash: group.hard_boundary_hash,
        representative_event_key: representative.item.event_key.clone(),
        representative_selection: REPRESENTATIVE_SELECTION.to_string(),
        grouping_keys: group.grouping_keys,
        member_count: members.len() as u64,
        occurrence_summary,
        review_patch_ids,
        members,
    })
}

#[derive(Serialize)]
struct GroupIdMaterial<'a> {
    version: &'static str,
    group_key_hashes: &'a [String],
    member_event_keys: &'a [&'a str],
    review_patch_ids: &'a [String],
}

fn unique_group_keys<'a>(members: impl Iterator<Item = &'a KeyedItem>) -> Vec<UnresolvedGroupKey> {
    let mut by_hash = BTreeMap::new();
    for member in members {
        by_hash.insert(member.group_key_hash.clone(), member.group_key.clone());
    }
    by_hash.into_values().collect()
}

fn unique_key_hashes<'a>(members: impl Iterator<Item = &'a KeyedItem>) -> Vec<String> {
    members
        .map(|member| member.group_key_hash.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn group_occurrence_summary(members: &[UnresolvedGroupMember]) -> OccurrenceSummary {
    let mut projects = BTreeSet::new();
    let mut runs = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut total = 0;

    for member in members {
        total += member.occurrences.len() as u64;
        for occurrence in &member.occurrences {
            projects.insert(occurrence.project_ref.clone());
            runs.insert(occurrence.run_ref.clone());
            sources.insert(occurrence.source_ref.clone());
        }
    }

    OccurrenceSummary {
        total_occurrences: total,
        distinct_projects: projects.len() as u64,
        distinct_runs: runs.len() as u64,
        distinct_sources: sources.len() as u64,
    }
}

fn build_summary(artifact: &UnresolvedGroupsArtifact) -> UnresolvedGroupsSummary {
    let mut summary = UnresolvedGroupsSummary {
        total_groups: artifact.groups.len() as u64,
        total_members: artifact.groups.iter().map(|group| group.member_count).sum(),
        total_occurrences: artifact
            .groups
            .iter()
            .map(|group| group.occurrence_summary.total_occurrences)
            .sum(),
        reviewed_patch_count: artifact.plan.review_patches.len() as u64,
        by_reason_code: BTreeMap::new(),
        by_field_role: BTreeMap::new(),
    };

    for group in &artifact.groups {
        for member in &group.members {
            *summary
                .by_reason_code
                .entry(enum_name(member.reason_code))
                .or_default() += 1;
            *summary
                .by_field_role
                .entry(enum_name(member.field_role))
                .or_default() += 1;
        }
    }

    summary
}

fn cannot_pairs(rules: &[CannotGroupRule]) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for rule in rules {
        pairs.insert(event_pair(&rule.left_event_key, &rule.right_event_key));
    }
    pairs
}

fn event_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn normalize_string_list(values: &mut Vec<String>) {
    values.iter_mut().for_each(|value| {
        *value = value.trim().to_string();
    });
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

fn hash_without_self(artifact: &UnresolvedGroupsArtifact) -> InboxResult<String> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hash_serialize(&hashable)
}

fn hash_serialize(value: impl Serialize) -> InboxResult<String> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| artifact_contract_error(format!("failed to hash JSON value: {error}")))?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn normalized_hash(value: &str, field: &str) -> InboxResult<String> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 digest with 64 lowercase hex characters"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 digest with 64 lowercase hex characters"
        )));
    }
    Ok(value.to_string())
}

fn canonical_timestamp(value: &str, field: &str) -> InboxResult<String> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|error| {
            artifact_contract_error(format!("invalid RFC3339 timestamp for {field}: {error}"))
        })
}

fn keyed_item_cmp(left: &KeyedItem, right: &KeyedItem) -> std::cmp::Ordering {
    left.item
        .first_seen_at
        .cmp(&right.item.first_seen_at)
        .then_with(|| left.item.event_key.cmp(&right.item.event_key))
        .then_with(|| left.item.field_name.cmp(&right.item.field_name))
}

fn group_cmp(left: &UnresolvedGroup, right: &UnresolvedGroup) -> std::cmp::Ordering {
    left.group_id.cmp(&right.group_id).then_with(|| {
        left.representative_event_key
            .cmp(&right.representative_event_key)
    })
}

fn fingerprint_cmp(
    left: &NormalizedSurfaceFingerprint,
    right: &NormalizedSurfaceFingerprint,
) -> std::cmp::Ordering {
    left.normalizer_id
        .cmp(&right.normalizer_id)
        .then_with(|| left.surface_role.cmp(&right.surface_role))
        .then_with(|| left.fingerprint.cmp(&right.fingerprint))
}

fn cannot_rule_cmp(left: &CannotGroupRule, right: &CannotGroupRule) -> std::cmp::Ordering {
    left.left_event_key
        .cmp(&right.left_event_key)
        .then_with(|| left.right_event_key.cmp(&right.right_event_key))
        .then_with(|| left.rule_id.cmp(&right.rule_id))
}

fn review_patch_cmp(left: &GroupReviewPatch, right: &GroupReviewPatch) -> std::cmp::Ordering {
    left.patch_id
        .cmp(&right.patch_id)
        .then_with(|| left.action.cmp(&right.action))
        .then_with(|| left.reviewed_at.cmp(&right.reviewed_at))
}

fn enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn artifact_contract_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::ArtifactContract, message)
}

fn corrupt_reference_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::CorruptReference, message)
}
