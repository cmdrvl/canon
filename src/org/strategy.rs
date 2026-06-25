//! Strategy parsing and validation for `canon entity`.

use super::types::{
    EntityError, EntityErrorCode, EntityResult, EntitySideField, EntityState, EntityStrategy,
    NormalizeConfig, StrategyAnchors, StrategyEvidence, StrategyObservations, StrategyOperator,
    StrategyPromotion, StrategyReconcile, StrategySolver,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const SCORE_MODE_NAMESPACE_MAX_SUM: &str = "namespace_max_sum";
const COMPONENT_SCORE_MODE_CORE_BEST_PAIR_SUM: &str = "core_best_pair_sum";
const MERGE_POLICY_RECIPROCAL_BEST: &str = "reciprocal_best";
const RECONCILE_INHERIT: &str = "inherit";
const RECONCILE_ABSTAIN_CONFLICT: &str = "abstain_conflict";

pub fn load_strategy(path: &Path) -> EntityResult<EntityStrategy> {
    let bytes = fs::read(path).map_err(|error| {
        EntityError::with_detail(
            EntityErrorCode::Strategy,
            format!("Failed to read strategy file '{}'", path.display()),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    parse_strategy_bytes(&bytes)
}

pub fn parse_strategy_bytes(bytes: &[u8]) -> EntityResult<EntityStrategy> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        EntityError::with_detail(
            EntityErrorCode::Strategy,
            "Strategy YAML must be valid UTF-8",
            json!({
                "error": error.to_string(),
            }),
        )
    })?;

    let raw: RawEntityStrategy = serde_yaml::from_str(text).map_err(|error| {
        EntityError::with_detail(
            EntityErrorCode::Strategy,
            "Invalid org strategy YAML",
            json!({
                "error": error.to_string(),
            }),
        )
    })?;

    let strategy = EntityStrategy {
        id: raw.id,
        version: raw.version,
        entity_type: raw.entity_type,
        description: raw.description,
        id_prefix: raw.id_prefix,
        observations: StrategyObservations {
            name_fields: raw.observations.name_fields,
            required_side_fields: raw.observations.required_side_fields,
            context_fields: raw.observations.context_fields,
            anchor_fields: raw.observations.anchor_fields,
        },
        normalize: NormalizeConfig {
            views: raw.normalize.views,
        },
        blocking: raw.blocking,
        evidence: StrategyEvidence {
            must_link: raw.evidence.must_link,
            support: raw.evidence.support,
            cannot_link: raw.evidence.cannot_link,
        },
        solver: StrategySolver {
            score_mode: raw.solver.score_mode,
            component_score_mode: raw.solver.component_score_mode,
            merge_policy: raw.solver.merge_policy,
            backbone_score_min: raw.solver.backbone_score_min,
            backbone_requires_positive_name: raw.solver.backbone_requires_positive_name,
            attach_score_min: raw.solver.attach_score_min,
            abstain_margin: raw.solver.abstain_margin,
            max_cluster_diameter: raw.solver.max_cluster_diameter,
            require_positive_name_evidence: raw.solver.require_positive_name_evidence,
            attach_requires_backbone_contact: raw.solver.attach_requires_backbone_contact,
            score_against_backbone_only: raw.solver.score_against_backbone_only,
            attachments_do_not_chain: raw.solver.attachments_do_not_chain,
        },
        reconcile: StrategyReconcile {
            single_incumbent_overlap: raw.reconcile.single_incumbent_overlap,
            multi_incumbent_overlap: raw.reconcile.multi_incumbent_overlap,
            allow_incumbent_merge: raw.reconcile.allow_incumbent_merge,
            allow_alias_writeback_for_resolved_existing: raw
                .reconcile
                .allow_alias_writeback_for_resolved_existing,
        },
        anchors: StrategyAnchors {
            precedence: raw.anchors.precedence,
            trusted_for_must_link: raw.anchors.trusted_for_must_link,
            trusted_for_single_doc_promotion: raw.anchors.trusted_for_single_doc_promotion,
            support_only: raw.anchors.support_only,
            require_unique_for_attachment: raw.anchors.require_unique_for_attachment,
        },
        promotion: StrategyPromotion {
            write_states: raw.promotion.write_states,
            require_zero_anchor_conflicts: raw.promotion.require_zero_anchor_conflicts,
            require_holdout_non_regression: raw.promotion.require_holdout_non_regression,
            require_perturbation_stability_gte: raw.promotion.require_perturbation_stability_gte,
            min_distinct_docs: raw.promotion.min_distinct_docs,
            allow_single_doc_if_unique_anchor: raw.promotion.allow_single_doc_if_unique_anchor,
        },
        content_hash: strategy_content_hash(bytes),
    };

    validate_strategy(&strategy)?;
    Ok(strategy)
}

pub fn strategy_content_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn validate_strategy(strategy: &EntityStrategy) -> EntityResult<()> {
    validate_non_empty("strategy_id", &strategy.id)?;
    validate_non_empty("strategy_version", &strategy.version)?;
    validate_non_empty("entity_type", &strategy.entity_type)?;
    validate_non_empty("id_prefix", &strategy.id_prefix)?;

    validate_observations(strategy)?;
    validate_normalize(strategy)?;
    validate_blocking(strategy)?;
    validate_evidence(strategy)?;
    validate_solver(strategy)?;
    validate_reconcile(strategy)?;
    validate_anchors(strategy)?;
    validate_promotion(strategy)?;

    Ok(())
}

fn validate_observations(strategy: &EntityStrategy) -> EntityResult<()> {
    if strategy.observations.name_fields.is_empty() {
        return Err(strategy_error(
            "observations.name_fields must contain at least one field",
            json!({ "name_fields": strategy.observations.name_fields }),
        ));
    }

    validate_unique_non_empty_strings(
        "observations.name_fields",
        &strategy.observations.name_fields,
    )?;
    validate_unique_non_empty_strings(
        "observations.context_fields",
        &strategy.observations.context_fields,
    )?;

    let mut required_side_fields = BTreeSet::new();
    for field in &strategy.observations.required_side_fields {
        if !required_side_fields.insert(*field) {
            return Err(strategy_error(
                "observations.required_side_fields must not contain duplicates",
                json!({ "required_side_field": field }),
            ));
        }
    }

    if strategy.observations.anchor_fields.is_empty() {
        return Err(strategy_error(
            "observations.anchor_fields must contain at least one anchor mapping",
            json!({ "anchor_fields": strategy.observations.anchor_fields }),
        ));
    }

    let mut anchor_namespaces = BTreeSet::new();
    let mut anchor_columns = BTreeSet::new();
    for (namespace, field) in &strategy.observations.anchor_fields {
        validate_non_empty("observations.anchor_fields namespace", namespace)?;
        validate_non_empty("observations.anchor_fields field", field)?;

        if !anchor_namespaces.insert(namespace.as_str()) {
            return Err(strategy_error(
                "observations.anchor_fields must not repeat namespaces",
                json!({ "namespace": namespace }),
            ));
        }
        if !anchor_columns.insert(field.as_str()) {
            return Err(strategy_error(
                "observations.anchor_fields must not map multiple namespaces to the same field",
                json!({ "field": field }),
            ));
        }
    }

    Ok(())
}

fn validate_normalize(strategy: &EntityStrategy) -> EntityResult<()> {
    if strategy.normalize.views.is_empty() {
        return Err(strategy_error(
            "normalize.views must contain at least one named view",
            json!({ "views": strategy.normalize.views }),
        ));
    }

    for (view_name, operators) in &strategy.normalize.views {
        validate_non_empty("normalize.views key", view_name)?;
        if operators.is_empty() {
            return Err(strategy_error(
                "normalize view pipelines must not be empty",
                json!({ "view": view_name }),
            ));
        }

        for operator in operators {
            validate_non_empty("normalize operator", operator)?;
            if !is_supported_normalize_operator(operator) {
                return Err(strategy_error(
                    "Unsupported normalize operator for validated BDC v1 strategy",
                    json!({
                        "view": view_name,
                        "operator": operator,
                    }),
                ));
            }
        }
    }

    Ok(())
}

fn validate_blocking(strategy: &EntityStrategy) -> EntityResult<()> {
    for (index, operator) in strategy.blocking.iter().enumerate() {
        let path = format!("blocking[{index}]");
        match operator.op.as_str() {
            "exact_view" => {
                reject_extra_params(operator, &path, &["view"])?;
                require_view_param(strategy, operator, &path, "view")?;
            }
            "rare_token_overlap" => {
                reject_extra_params(
                    operator,
                    &path,
                    &["left_view", "right_view", "min_tokens", "min_idf"],
                )?;
                require_view_param(strategy, operator, &path, "left_view")?;
                require_view_param(strategy, operator, &path, "right_view")?;
                require_positive_u64_param(operator, &path, "min_tokens")?;
                require_non_negative_f64_param(operator, &path, "min_idf")?;
            }
            "shared_anchor" => {
                reject_extra_params(operator, &path, &["anchor"])?;
                require_anchor_param(strategy, operator, &path, "anchor")?;
            }
            "registry_alias_match" => {
                reject_extra_params(operator, &path, &[])?;
            }
            other => {
                return Err(strategy_error(
                    "Unsupported blocking operator for validated BDC v1 strategy",
                    json!({
                        "path": path,
                        "operator": other,
                    }),
                ));
            }
        }
    }

    Ok(())
}

fn validate_evidence(strategy: &EntityStrategy) -> EntityResult<()> {
    validate_evidence_group(strategy, "evidence.must_link", &strategy.evidence.must_link)?;
    validate_evidence_group(strategy, "evidence.support", &strategy.evidence.support)?;
    validate_evidence_group(
        strategy,
        "evidence.cannot_link",
        &strategy.evidence.cannot_link,
    )?;
    Ok(())
}

fn validate_evidence_group(
    strategy: &EntityStrategy,
    group: &str,
    operators: &[StrategyOperator],
) -> EntityResult<()> {
    for (index, operator) in operators.iter().enumerate() {
        let path = format!("{group}[{index}]");
        match group {
            "evidence.must_link" => validate_must_link_operator(strategy, operator, &path)?,
            "evidence.support" => validate_support_operator(strategy, operator, &path)?,
            "evidence.cannot_link" => validate_cannot_link_operator(strategy, operator, &path)?,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn validate_must_link_operator(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
) -> EntityResult<()> {
    match operator.op.as_str() {
        "shared_anchor" => {
            reject_extra_params(operator, path, &["anchor"])?;
            require_anchor_param(strategy, operator, path, "anchor")?;
        }
        "registry_alias_match" => {
            reject_extra_params(operator, path, &[])?;
        }
        other => {
            return Err(strategy_error(
                "Unsupported must-link operator for validated BDC v1 strategy",
                json!({
                    "path": path,
                    "operator": other,
                }),
            ));
        }
    }

    Ok(())
}

fn validate_support_operator(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
) -> EntityResult<()> {
    match operator.op.as_str() {
        "exact_view" => {
            reject_extra_params(operator, path, &["view", "score"])?;
            require_view_param(strategy, operator, path, "view")?;
            require_positive_i64_param(operator, path, "score")?;
        }
        "acronym_plus_token" => {
            reject_extra_params(operator, path, &["acronym_view", "token_view", "score"])?;
            require_view_param(strategy, operator, path, "acronym_view")?;
            require_view_param(strategy, operator, path, "token_view")?;
            require_positive_i64_param(operator, path, "score")?;
        }
        "categorical_equal" => {
            reject_extra_params(operator, path, &["field", "score"])?;
            require_context_field_param(strategy, operator, path, "field")?;
            require_positive_i64_param(operator, path, "score")?;
        }
        other => {
            return Err(strategy_error(
                "Unsupported support operator for validated BDC v1 strategy",
                json!({
                    "path": path,
                    "operator": other,
                }),
            ));
        }
    }

    Ok(())
}

fn validate_cannot_link_operator(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
) -> EntityResult<()> {
    match operator.op.as_str() {
        "conflicting_anchor" => {
            reject_extra_params(operator, path, &["anchor"])?;
            require_anchor_param(strategy, operator, path, "anchor")?;
        }
        other => {
            return Err(strategy_error(
                "Unsupported cannot-link operator for validated BDC v1 strategy",
                json!({
                    "path": path,
                    "operator": other,
                }),
            ));
        }
    }

    Ok(())
}

fn validate_solver(strategy: &EntityStrategy) -> EntityResult<()> {
    if strategy.solver.score_mode != SCORE_MODE_NAMESPACE_MAX_SUM {
        return Err(strategy_error(
            "solver.score_mode must match the validated BDC v1 policy",
            json!({ "score_mode": strategy.solver.score_mode }),
        ));
    }
    if strategy.solver.component_score_mode != COMPONENT_SCORE_MODE_CORE_BEST_PAIR_SUM {
        return Err(strategy_error(
            "solver.component_score_mode must match the validated BDC v1 policy",
            json!({ "component_score_mode": strategy.solver.component_score_mode }),
        ));
    }
    if strategy.solver.merge_policy != MERGE_POLICY_RECIPROCAL_BEST {
        return Err(strategy_error(
            "solver.merge_policy must match the validated BDC v1 policy",
            json!({ "merge_policy": strategy.solver.merge_policy }),
        ));
    }

    if strategy.solver.backbone_score_min < 0
        || strategy.solver.attach_score_min < 0
        || strategy.solver.abstain_margin < 0
    {
        return Err(strategy_error(
            "solver thresholds must be non-negative",
            json!({
                "backbone_score_min": strategy.solver.backbone_score_min,
                "attach_score_min": strategy.solver.attach_score_min,
                "abstain_margin": strategy.solver.abstain_margin,
            }),
        ));
    }
    if strategy.solver.max_cluster_diameter == 0 {
        return Err(strategy_error(
            "solver.max_cluster_diameter must be greater than zero",
            json!({ "max_cluster_diameter": strategy.solver.max_cluster_diameter }),
        ));
    }

    let required_true_flags = [
        (
            "backbone_requires_positive_name",
            strategy.solver.backbone_requires_positive_name,
        ),
        (
            "require_positive_name_evidence",
            strategy.solver.require_positive_name_evidence,
        ),
        (
            "attach_requires_backbone_contact",
            strategy.solver.attach_requires_backbone_contact,
        ),
        (
            "score_against_backbone_only",
            strategy.solver.score_against_backbone_only,
        ),
        (
            "attachments_do_not_chain",
            strategy.solver.attachments_do_not_chain,
        ),
    ];

    for (name, enabled) in required_true_flags {
        if !enabled {
            return Err(strategy_error(
                "Validated BDC v1 solver safety flags must remain enabled",
                json!({ "field": name, "value": enabled }),
            ));
        }
    }

    Ok(())
}

fn validate_reconcile(strategy: &EntityStrategy) -> EntityResult<()> {
    if strategy.reconcile.single_incumbent_overlap != RECONCILE_INHERIT {
        return Err(strategy_error(
            "reconcile.single_incumbent_overlap must be 'inherit'",
            json!({
                "single_incumbent_overlap": strategy.reconcile.single_incumbent_overlap,
            }),
        ));
    }
    if strategy.reconcile.multi_incumbent_overlap != RECONCILE_ABSTAIN_CONFLICT {
        return Err(strategy_error(
            "reconcile.multi_incumbent_overlap must be 'abstain_conflict'",
            json!({
                "multi_incumbent_overlap": strategy.reconcile.multi_incumbent_overlap,
            }),
        ));
    }
    if strategy.reconcile.allow_incumbent_merge {
        return Err(strategy_error(
            "reconcile.allow_incumbent_merge must remain false for validated BDC v1",
            json!({
                "allow_incumbent_merge": strategy.reconcile.allow_incumbent_merge,
            }),
        ));
    }

    Ok(())
}

fn validate_anchors(strategy: &EntityStrategy) -> EntityResult<()> {
    let declared_anchors = strategy
        .observations
        .anchor_fields
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();

    validate_unique_non_empty_strings("anchors.precedence", &strategy.anchors.precedence)?;
    validate_unique_non_empty_strings(
        "anchors.trusted_for_must_link",
        &strategy.anchors.trusted_for_must_link,
    )?;
    validate_unique_non_empty_strings(
        "anchors.trusted_for_single_doc_promotion",
        &strategy.anchors.trusted_for_single_doc_promotion,
    )?;
    validate_unique_non_empty_strings("anchors.support_only", &strategy.anchors.support_only)?;

    for anchor in strategy
        .anchors
        .precedence
        .iter()
        .chain(strategy.anchors.trusted_for_must_link.iter())
        .chain(strategy.anchors.trusted_for_single_doc_promotion.iter())
        .chain(strategy.anchors.support_only.iter())
    {
        if !declared_anchors.contains(anchor) {
            return Err(strategy_error(
                "Anchor trust configuration references an undeclared anchor namespace",
                json!({
                    "anchor": anchor,
                    "declared_anchors": declared_anchors,
                }),
            ));
        }
    }

    if !strategy
        .anchors
        .trusted_for_single_doc_promotion
        .iter()
        .all(|anchor| strategy.anchors.trusted_for_must_link.contains(anchor))
    {
        return Err(strategy_error(
            "anchors.trusted_for_single_doc_promotion must be a subset of anchors.trusted_for_must_link",
            json!({
                "trusted_for_single_doc_promotion": strategy.anchors.trusted_for_single_doc_promotion,
                "trusted_for_must_link": strategy.anchors.trusted_for_must_link,
            }),
        ));
    }

    let trusted = strategy
        .anchors
        .trusted_for_must_link
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let support_only = strategy
        .anchors
        .support_only
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let overlap = trusted
        .intersection(&support_only)
        .cloned()
        .collect::<Vec<_>>();
    if !overlap.is_empty() {
        return Err(strategy_error(
            "anchors.trusted_for_must_link and anchors.support_only must be disjoint",
            json!({ "overlap": overlap }),
        ));
    }

    let referenced_anchors = referenced_anchor_names(strategy);
    for anchor in referenced_anchors {
        if !strategy.anchors.precedence.contains(&anchor) {
            return Err(strategy_error(
                "anchors.precedence must include every anchor namespace referenced by the strategy",
                json!({
                    "anchor": anchor,
                    "precedence": strategy.anchors.precedence,
                }),
            ));
        }
    }

    Ok(())
}

fn validate_promotion(strategy: &EntityStrategy) -> EntityResult<()> {
    if strategy.promotion.write_states.is_empty() {
        return Err(strategy_error(
            "promotion.write_states must contain at least one state",
            json!({ "write_states": strategy.promotion.write_states }),
        ));
    }

    let mut write_states = Vec::new();
    for state in &strategy.promotion.write_states {
        if write_states.contains(state) {
            return Err(strategy_error(
                "promotion.write_states must not contain duplicates",
                json!({ "write_state": state }),
            ));
        }
        write_states.push(*state);
    }

    if !strategy.promotion.require_zero_anchor_conflicts {
        return Err(strategy_error(
            "promotion.require_zero_anchor_conflicts must remain enabled",
            json!({
                "require_zero_anchor_conflicts": strategy.promotion.require_zero_anchor_conflicts,
            }),
        ));
    }
    if !strategy.promotion.require_holdout_non_regression {
        return Err(strategy_error(
            "promotion.require_holdout_non_regression must remain enabled",
            json!({
                "require_holdout_non_regression": strategy.promotion.require_holdout_non_regression,
            }),
        ));
    }
    if !(0.0..=1.0).contains(&strategy.promotion.require_perturbation_stability_gte) {
        return Err(strategy_error(
            "promotion.require_perturbation_stability_gte must be between 0.0 and 1.0",
            json!({
                "require_perturbation_stability_gte": strategy.promotion.require_perturbation_stability_gte,
            }),
        ));
    }
    if strategy.promotion.min_distinct_docs == 0 {
        return Err(strategy_error(
            "promotion.min_distinct_docs must be greater than zero",
            json!({ "min_distinct_docs": strategy.promotion.min_distinct_docs }),
        ));
    }

    Ok(())
}

fn referenced_anchor_names(strategy: &EntityStrategy) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();

    for operator in &strategy.blocking {
        if let Some(anchor) = string_param(operator, "anchor") {
            anchors.insert(anchor.to_string());
        }
    }
    for group in [
        &strategy.evidence.must_link,
        &strategy.evidence.support,
        &strategy.evidence.cannot_link,
    ] {
        for operator in group {
            if let Some(anchor) = string_param(operator, "anchor") {
                anchors.insert(anchor.to_string());
            }
        }
    }

    anchors
}

fn reject_extra_params(
    operator: &StrategyOperator,
    path: &str,
    allowed: &[&str],
) -> EntityResult<()> {
    let allowed_keys = allowed.iter().copied().collect::<BTreeSet<_>>();
    let extras = operator
        .params
        .keys()
        .filter(|key| !allowed_keys.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if extras.is_empty() {
        return Ok(());
    }

    Err(strategy_error(
        "Strategy operator includes unsupported parameters",
        json!({
            "path": path,
            "operator": operator.op,
            "unsupported_params": extras,
            "allowed_params": allowed,
        }),
    ))
}

fn require_view_param(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<String> {
    let view = require_string_param(operator, path, key)?;
    if !strategy.normalize.views.contains_key(&view) {
        return Err(strategy_error(
            "Strategy operator references an unknown normalize view",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "view": view,
                "known_views": strategy.normalize.views.keys().collect::<Vec<_>>(),
            }),
        ));
    }
    Ok(view)
}

fn require_anchor_param(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<String> {
    let anchor = require_string_param(operator, path, key)?;
    if !strategy.observations.anchor_fields.contains_key(&anchor) {
        return Err(strategy_error(
            "Strategy operator references an unknown anchor namespace",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "anchor": anchor,
                "known_anchors": strategy.observations.anchor_fields.keys().collect::<Vec<_>>(),
            }),
        ));
    }
    Ok(anchor)
}

fn require_context_field_param(
    strategy: &EntityStrategy,
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<String> {
    let field = require_string_param(operator, path, key)?;
    if !strategy.observations.context_fields.contains(&field) {
        return Err(strategy_error(
            "Strategy operator references an unknown context field",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "field": field,
                "known_context_fields": strategy.observations.context_fields,
            }),
        ));
    }
    Ok(field)
}

fn require_string_param(
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<String> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Strategy operator is missing a required parameter",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
            }),
        )
    })?;

    let string_value = value.as_str().ok_or_else(|| {
        strategy_error(
            "Strategy operator parameter must be a string",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        )
    })?;
    validate_non_empty(key, string_value)?;
    Ok(string_value.to_string())
}

fn require_positive_u64_param(
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<u64> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Strategy operator is missing a required parameter",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
            }),
        )
    })?;

    let numeric = value.as_u64().ok_or_else(|| {
        strategy_error(
            "Strategy operator parameter must be a positive integer",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        )
    })?;

    if numeric == 0 {
        return Err(strategy_error(
            "Strategy operator parameter must be greater than zero",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        ));
    }

    Ok(numeric)
}

fn require_positive_i64_param(
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<i64> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Strategy operator is missing a required parameter",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
            }),
        )
    })?;

    let numeric = value.as_i64().ok_or_else(|| {
        strategy_error(
            "Strategy operator parameter must be a positive integer",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        )
    })?;

    if numeric <= 0 {
        return Err(strategy_error(
            "Strategy operator parameter must be greater than zero",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        ));
    }

    Ok(numeric)
}

fn require_non_negative_f64_param(
    operator: &StrategyOperator,
    path: &str,
    key: &str,
) -> EntityResult<f64> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Strategy operator is missing a required parameter",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
            }),
        )
    })?;

    let numeric = value.as_f64().ok_or_else(|| {
        strategy_error(
            "Strategy operator parameter must be numeric",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        )
    })?;

    if numeric < 0.0 {
        return Err(strategy_error(
            "Strategy operator parameter must be non-negative",
            json!({
                "path": path,
                "operator": operator.op,
                "parameter": key,
                "value": value,
            }),
        ));
    }

    Ok(numeric)
}

fn string_param<'a>(operator: &'a StrategyOperator, key: &str) -> Option<&'a str> {
    operator.params.get(key).and_then(Value::as_str)
}

fn validate_unique_non_empty_strings(path: &str, values: &[String]) -> EntityResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_non_empty(path, value)?;
        if !seen.insert(value.as_str()) {
            return Err(strategy_error(
                "List entries must be unique",
                json!({
                    "path": path,
                    "value": value,
                }),
            ));
        }
    }
    Ok(())
}

fn validate_non_empty(path: &str, value: &str) -> EntityResult<()> {
    if value.trim().is_empty() {
        return Err(strategy_error(
            "Required strategy value must not be empty",
            json!({
                "path": path,
                "value": value,
            }),
        ));
    }

    Ok(())
}

fn strategy_error(message: impl Into<String>, detail: Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::Strategy, message, detail)
}

fn is_supported_normalize_operator(operator: &str) -> bool {
    matches!(
        operator,
        "lowercase"
            | "ascii_trim"
            | "strip_footnotes"
            | "strip_parenthetical_notes"
            | "strip_legal_suffixes"
            | "normalize_whitespace"
            | "extract_initials"
            | "tokenize"
            | "drop_stopwords"
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntityStrategy {
    #[serde(rename = "strategy_id")]
    id: String,
    #[serde(rename = "strategy_version")]
    version: String,
    entity_type: String,
    #[serde(default)]
    description: String,
    id_prefix: String,
    observations: RawStrategyObservations,
    normalize: RawNormalizeConfig,
    #[serde(default)]
    blocking: Vec<StrategyOperator>,
    evidence: RawStrategyEvidence,
    solver: RawStrategySolver,
    reconcile: RawStrategyReconcile,
    anchors: RawStrategyAnchors,
    promotion: RawStrategyPromotion,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategyObservations {
    #[serde(default)]
    name_fields: Vec<String>,
    #[serde(default)]
    required_side_fields: Vec<EntitySideField>,
    #[serde(default)]
    context_fields: Vec<String>,
    #[serde(default)]
    anchor_fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNormalizeConfig {
    #[serde(default)]
    views: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategyEvidence {
    #[serde(default)]
    must_link: Vec<StrategyOperator>,
    #[serde(default)]
    support: Vec<StrategyOperator>,
    #[serde(default)]
    cannot_link: Vec<StrategyOperator>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategySolver {
    score_mode: String,
    component_score_mode: String,
    merge_policy: String,
    backbone_score_min: i64,
    backbone_requires_positive_name: bool,
    attach_score_min: i64,
    abstain_margin: i64,
    max_cluster_diameter: u32,
    require_positive_name_evidence: bool,
    attach_requires_backbone_contact: bool,
    score_against_backbone_only: bool,
    attachments_do_not_chain: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategyReconcile {
    single_incumbent_overlap: String,
    multi_incumbent_overlap: String,
    allow_incumbent_merge: bool,
    allow_alias_writeback_for_resolved_existing: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategyAnchors {
    #[serde(default)]
    precedence: Vec<String>,
    #[serde(default)]
    trusted_for_must_link: Vec<String>,
    #[serde(default)]
    trusted_for_single_doc_promotion: Vec<String>,
    #[serde(default)]
    support_only: Vec<String>,
    require_unique_for_attachment: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStrategyPromotion {
    #[serde(default)]
    write_states: Vec<EntityState>,
    require_zero_anchor_conflicts: bool,
    require_holdout_non_regression: bool,
    require_perturbation_stability_gte: f64,
    min_distinct_docs: u32,
    allow_single_doc_if_unique_anchor: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_STRATEGY: &str = r#"
strategy_id: bdc_org_graph.v1
strategy_version: "0.1.0"
entity_type: issuer
description: "Resolve BDC portfolio-company identities via constrained evidence graph"
id_prefix: "IC"

observations:
  name_fields: [portfolio_company]
  required_side_fields: [alias_surfaces_json]
  context_fields: [industry, investment_type]
  anchor_fields:
    lei: lei
    figi: figi
    cik: cik

normalize:
  views:
    core_name:
      - lowercase
      - strip_footnotes
      - strip_legal_suffixes
      - normalize_whitespace
    acronym:
      - extract_initials
    rare_tokens:
      - tokenize
      - drop_stopwords

blocking:
  - op: exact_view
    view: core_name
  - op: rare_token_overlap
    left_view: rare_tokens
    right_view: rare_tokens
    min_tokens: 2
    min_idf: 4.0
  - op: shared_anchor
    anchor: lei
  - op: registry_alias_match

evidence:
  must_link:
    - op: shared_anchor
      anchor: lei
    - op: registry_alias_match
  support:
    - op: exact_view
      view: core_name
      score: 32
    - op: acronym_plus_token
      acronym_view: acronym
      token_view: rare_tokens
      score: 10
    - op: categorical_equal
      field: industry
      score: 4
  cannot_link:
    - op: conflicting_anchor
      anchor: lei

solver:
  score_mode: namespace_max_sum
  component_score_mode: core_best_pair_sum
  merge_policy: reciprocal_best
  backbone_score_min: 32
  backbone_requires_positive_name: true
  attach_score_min: 28
  abstain_margin: 6
  max_cluster_diameter: 2
  require_positive_name_evidence: true
  attach_requires_backbone_contact: true
  score_against_backbone_only: true
  attachments_do_not_chain: true

reconcile:
  single_incumbent_overlap: inherit
  multi_incumbent_overlap: abstain_conflict
  allow_incumbent_merge: false
  allow_alias_writeback_for_resolved_existing: true

anchors:
  precedence: [lei, cik, figi]
  trusted_for_must_link: [lei]
  trusted_for_single_doc_promotion: [lei]
  support_only: [cik, figi]
  require_unique_for_attachment: true

promotion:
  write_states: [PROMOTABLE_NEW, RESOLVED_EXISTING]
  require_zero_anchor_conflicts: true
  require_holdout_non_regression: true
  require_perturbation_stability_gte: 0.995
  min_distinct_docs: 2
  allow_single_doc_if_unique_anchor: true
"#;

    #[test]
    fn parses_valid_bdc_org_strategy() {
        let strategy = parse_strategy_bytes(VALID_STRATEGY.as_bytes()).expect("valid strategy");

        assert_eq!(strategy.id, "bdc_org_graph.v1");
        assert_eq!(strategy.version, "0.1.0");
        assert_eq!(strategy.observations.name_fields, vec!["portfolio_company"]);
        assert_eq!(
            strategy.observations.required_side_fields,
            vec![EntitySideField::AliasSurfacesJson]
        );
        assert_eq!(
            strategy
                .blocking
                .iter()
                .map(|op| op.op.as_str())
                .collect::<Vec<_>>(),
            vec![
                "exact_view",
                "rare_token_overlap",
                "shared_anchor",
                "registry_alias_match"
            ]
        );
        assert_eq!(
            strategy.content_hash,
            strategy_content_hash(VALID_STRATEGY.as_bytes())
        );
    }

    #[test]
    fn rejects_unsupported_blocking_operator() {
        let invalid = VALID_STRATEGY.replace("op: shared_anchor", "op: prefix_view");
        let error = parse_strategy_bytes(invalid.as_bytes()).expect_err("invalid strategy");

        assert_eq!(error.code, EntityErrorCode::Strategy);
        assert!(error.message.contains("Unsupported blocking operator"));
    }

    #[test]
    fn rejects_invalid_operator_parameter_shape() {
        let invalid = VALID_STRATEGY.replace("      score: 32\n", "      score: nope\n");
        let error = parse_strategy_bytes(invalid.as_bytes()).expect_err("invalid strategy");

        assert_eq!(error.code, EntityErrorCode::Strategy);
        assert!(error.message.contains("positive integer"));
    }

    #[test]
    fn rejects_invalid_anchor_trust_configuration() {
        let invalid = VALID_STRATEGY.replace(
            "  support_only: [cik, figi]\n",
            "  support_only: [lei, figi]\n",
        );
        let error = parse_strategy_bytes(invalid.as_bytes()).expect_err("invalid strategy");

        assert_eq!(error.code, EntityErrorCode::Strategy);
        assert!(error.message.contains("must be disjoint"));
    }

    #[test]
    fn rejects_invalid_required_side_field_value() {
        let invalid = VALID_STRATEGY.replace(
            "  required_side_fields: [alias_surfaces_json]\n",
            "  required_side_fields: [bogus_side_field]\n",
        );
        let error = parse_strategy_bytes(invalid.as_bytes()).expect_err("invalid strategy");

        assert_eq!(error.code, EntityErrorCode::Strategy);
        assert!(error.message.contains("Invalid org strategy YAML"));
    }

    #[test]
    fn content_hash_is_stable_for_identical_bytes() {
        let left = parse_strategy_bytes(VALID_STRATEGY.as_bytes()).expect("left strategy");
        let right = parse_strategy_bytes(VALID_STRATEGY.as_bytes()).expect("right strategy");

        assert_eq!(left.content_hash, right.content_hash);
    }
}
