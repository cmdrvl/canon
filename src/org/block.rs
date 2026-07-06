//! Candidate-block generation for `canon entity`.

use super::types::{
    BlockHit, BlockRecord, EntityError, EntityErrorCode, EntityResult, EntityStrategy,
    IncumbentMemory, ProjectedObservation,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const LEGAL_SUFFIXES: &[&str] = &[
    "co",
    "company",
    "corp",
    "corporation",
    "inc",
    "incorporated",
    "limited",
    "llc",
    "lp",
    "ltd",
    "plc",
];

const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "and",
    "capital",
    "company",
    "corp",
    "corporation",
    "group",
    "holdings",
    "inc",
    "incorporated",
    "limited",
    "llc",
    "lp",
    "ltd",
    "management",
    "of",
    "partners",
    "plc",
    "the",
];

#[derive(Debug, Clone, Default)]
struct PreparedObservation {
    row_id: String,
    view_values: BTreeMap<String, BTreeSet<String>>,
    view_tokens: BTreeMap<String, BTreeSet<String>>,
    anchors: BTreeMap<String, BTreeSet<String>>,
    registry_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBlockEstimate {
    pub total_pair_expansions: u64,
    pub max_operator_pair_expansions: u64,
    pub max_bucket_pair_expansions: u64,
    pub max_bucket_row_count: u64,
    pub max_bucket_operator_id: String,
    pub max_bucket_value: String,
}

pub fn build_candidate_blocks(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    incumbent: &IncumbentMemory,
) -> EntityResult<Vec<BlockRecord>> {
    let prepared = prepare_observations(strategy, observations, incumbent);
    let mut pair_hits: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for operator in &strategy.blocking {
        match operator.op.as_str() {
            "exact_view" => apply_exact_view(strategy, operator, &prepared, &mut pair_hits)?,
            "rare_token_overlap" => {
                apply_rare_token_overlap(strategy, operator, &prepared, &mut pair_hits)?
            }
            "shared_anchor" => apply_shared_anchor(strategy, operator, &prepared, &mut pair_hits)?,
            "registry_alias_match" => {
                apply_registry_alias_match(operator, &prepared, &mut pair_hits)
            }
            other => {
                return Err(strategy_error(
                    "Unsupported blocking operator for block generation",
                    json!({ "operator": other }),
                ));
            }
        }
    }

    Ok(pair_hits
        .into_iter()
        .map(|((left_row_id, right_row_id), hits)| BlockRecord {
            version: super::types::CANON_ENTITY_BLOCK_VERSION.to_string(),
            strategy: strategy.reference(),
            registry_snapshot: incumbent.registry.clone(),
            left_row_id,
            right_row_id,
            block_hits: hits
                .into_iter()
                .map(|operator_id| BlockHit { operator_id })
                .collect(),
        })
        .collect())
}

pub fn estimate_candidate_block_pairs(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    incumbent: &IncumbentMemory,
) -> EntityResult<CandidateBlockEstimate> {
    let prepared = prepare_observations(strategy, observations, incumbent);
    let mut estimate = CandidateBlockEstimate {
        total_pair_expansions: 0,
        max_operator_pair_expansions: 0,
        max_bucket_pair_expansions: 0,
        max_bucket_row_count: 0,
        max_bucket_operator_id: String::new(),
        max_bucket_value: String::new(),
    };

    for operator in &strategy.blocking {
        let operator_id = match operator.op.as_str() {
            "exact_view" => {
                let view = required_string_param(operator, "view")?;
                require_known_view(strategy, &view)?;
                let buckets = value_buckets_for_prepared(
                    &prepared,
                    format!("exact_view:{view}"),
                    |observation| observation.view_values.get(&view),
                );
                add_bucket_estimate(&mut estimate, buckets);
                continue;
            }
            "shared_anchor" => {
                let anchor = required_string_param(operator, "anchor")?;
                if !strategy.observations.anchor_fields.contains_key(&anchor) {
                    return Err(strategy_error(
                        "Blocking operator references an unknown anchor namespace",
                        json!({ "anchor": anchor }),
                    ));
                }
                let buckets = value_buckets_for_prepared(
                    &prepared,
                    format!("shared_anchor:{anchor}"),
                    |observation| observation.anchors.get(&anchor),
                );
                add_bucket_estimate(&mut estimate, buckets);
                continue;
            }
            "registry_alias_match" => "registry_alias_match".to_string(),
            "rare_token_overlap" => {
                let left_view = required_string_param(operator, "left_view")?;
                let right_view = required_string_param(operator, "right_view")?;
                require_known_view(strategy, &left_view)?;
                require_known_view(strategy, &right_view)?;
                format!("rare_token_overlap:{left_view}:{right_view}")
            }
            other => {
                return Err(strategy_error(
                    "Unsupported blocking operator for block generation",
                    json!({ "operator": other }),
                ));
            }
        };

        let operator_pairs = match operator.op.as_str() {
            "registry_alias_match" => {
                let buckets = value_buckets_for_prepared(&prepared, operator_id, |observation| {
                    Some(&observation.registry_ids)
                });
                add_bucket_estimate(&mut estimate, buckets);
                continue;
            }
            "rare_token_overlap" => choose_two(prepared.len() as u64),
            _ => 0,
        };
        estimate.total_pair_expansions = estimate
            .total_pair_expansions
            .saturating_add(operator_pairs);
        estimate.max_operator_pair_expansions =
            estimate.max_operator_pair_expansions.max(operator_pairs);
        if operator_pairs > estimate.max_bucket_pair_expansions {
            estimate.max_bucket_pair_expansions = operator_pairs;
            estimate.max_bucket_row_count = prepared.len() as u64;
            estimate.max_bucket_operator_id = operator_id;
            estimate.max_bucket_value = "<all_rows>".to_string();
        }
    }

    Ok(estimate)
}

fn value_buckets_for_prepared<'a, F>(
    prepared: &'a [PreparedObservation],
    operator_id: String,
    values: F,
) -> Vec<(String, String, u64)>
where
    F: Fn(&'a PreparedObservation) -> Option<&'a BTreeSet<String>>,
{
    let mut buckets = BTreeMap::<String, u64>::new();
    for observation in prepared {
        if let Some(values) = values(observation) {
            for value in values {
                *buckets.entry(value.clone()).or_default() += 1;
            }
        }
    }

    buckets
        .into_iter()
        .map(|(value, row_count)| (operator_id.clone(), value, row_count))
        .collect()
}

fn add_bucket_estimate(estimate: &mut CandidateBlockEstimate, buckets: Vec<(String, String, u64)>) {
    let mut operator_pairs = BTreeMap::<String, u64>::new();
    for (operator_id, value, row_count) in buckets {
        let bucket_pairs = choose_two(row_count);
        estimate.total_pair_expansions =
            estimate.total_pair_expansions.saturating_add(bucket_pairs);
        *operator_pairs.entry(operator_id.clone()).or_default() = operator_pairs
            .get(&operator_id)
            .copied()
            .unwrap_or_default()
            .saturating_add(bucket_pairs);

        if bucket_pairs > estimate.max_bucket_pair_expansions {
            estimate.max_bucket_pair_expansions = bucket_pairs;
            estimate.max_bucket_row_count = row_count;
            estimate.max_bucket_operator_id = operator_id;
            estimate.max_bucket_value = value;
        }
    }
    if let Some(operator_max) = operator_pairs.values().copied().max() {
        estimate.max_operator_pair_expansions =
            estimate.max_operator_pair_expansions.max(operator_max);
    }
}

fn choose_two(count: u64) -> u64 {
    count.saturating_mul(count.saturating_sub(1)) / 2
}

fn prepare_observations(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    incumbent: &IncumbentMemory,
) -> Vec<PreparedObservation> {
    let alias_index = build_alias_index(incumbent);

    observations
        .iter()
        .map(|observation| {
            let surfaces = observation_surfaces(observation);
            let view_values = normalize_views(strategy, &surfaces);
            let view_tokens = view_values
                .iter()
                .map(|(view_name, values)| {
                    let tokens = values
                        .iter()
                        .flat_map(|value| value.split_whitespace().map(ToOwned::to_owned))
                        .collect::<BTreeSet<_>>();
                    (view_name.clone(), tokens)
                })
                .collect::<BTreeMap<_, _>>();

            let anchors = observation
                .anchors
                .iter()
                .filter_map(|anchor| {
                    let value = trim_to_option(&anchor.value)?;
                    Some((anchor.namespace.clone(), value))
                })
                .fold(
                    BTreeMap::<String, BTreeSet<String>>::new(),
                    |mut acc, (namespace, value)| {
                        acc.entry(namespace).or_default().insert(value);
                        acc
                    },
                );

            let registry_ids = surfaces
                .iter()
                .filter_map(|surface| alias_index.get(surface))
                .flat_map(|ids| ids.iter().cloned())
                .collect::<BTreeSet<_>>();

            PreparedObservation {
                row_id: observation.source_row_id.clone(),
                view_values,
                view_tokens,
                anchors,
                registry_ids,
            }
        })
        .collect()
}

fn build_alias_index(incumbent: &IncumbentMemory) -> BTreeMap<String, BTreeSet<String>> {
    incumbent
        .alias_entries
        .iter()
        .filter_map(|entry| {
            let input = trim_to_option(&entry.input)?;
            let canonical_id = trim_to_option(&entry.canonical_id)?;
            Some((input, canonical_id))
        })
        .fold(
            BTreeMap::<String, BTreeSet<String>>::new(),
            |mut acc, (input, canonical_id)| {
                acc.entry(input).or_default().insert(canonical_id);
                acc
            },
        )
}

fn observation_surfaces(observation: &ProjectedObservation) -> BTreeSet<String> {
    std::iter::once(&observation.primary_surface)
        .chain(observation.alias_surfaces.iter())
        .chain(observation.mention_surfaces.iter())
        .filter_map(|surface| trim_to_option(&surface.value))
        .collect()
}

fn normalize_views(
    strategy: &EntityStrategy,
    surfaces: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    strategy
        .normalize
        .views
        .iter()
        .map(|(view_name, pipeline)| {
            let values = surfaces
                .iter()
                .filter_map(|surface| {
                    let normalized = apply_pipeline(surface, pipeline);
                    trim_to_option(&normalized)
                })
                .collect::<BTreeSet<_>>();
            (view_name.clone(), values)
        })
        .collect()
}

fn apply_pipeline(value: &str, pipeline: &[String]) -> String {
    let mut current = value.trim().to_string();

    for operator in pipeline {
        current = match operator.as_str() {
            "lowercase" => current.to_lowercase(),
            "strip_footnotes" => strip_footnotes(&current),
            "strip_legal_suffixes" => strip_legal_suffixes(&current),
            "normalize_whitespace" => normalize_whitespace(&current),
            "extract_initials" => extract_initials(&current),
            "tokenize" => tokenize(&current).join(" "),
            "drop_stopwords" => drop_stopwords(&current),
            _ => current,
        };
    }

    current.trim().to_string()
}

fn strip_footnotes(value: &str) -> String {
    let mut current = value.trim_end().to_string();

    loop {
        if !current.ends_with(')') {
            break;
        }

        let Some(open_idx) = current.rfind('(') else {
            break;
        };

        let inner = &current[open_idx + 1..current.len() - 1];
        if inner.is_empty() || !inner.chars().all(|ch| ch.is_ascii_digit()) {
            break;
        }

        current.truncate(open_idx);
        current = current.trim_end().to_string();
    }

    current
}

fn strip_legal_suffixes(value: &str) -> String {
    let mut tokens = value
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    while let Some(last) = tokens.last() {
        let normalized = normalize_token(last);
        if LEGAL_SUFFIXES.contains(&normalized.as_str()) {
            tokens.pop();
        } else {
            break;
        }
    }

    tokens.join(" ")
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_initials(value: &str) -> String {
    tokenize(value)
        .into_iter()
        .filter_map(|token| token.chars().next())
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn drop_stopwords(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|token| !STOPWORDS.contains(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_ascii_lowercase()
}

fn apply_exact_view(
    strategy: &EntityStrategy,
    operator: &super::types::StrategyOperator,
    observations: &[PreparedObservation],
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) -> EntityResult<()> {
    let view = required_string_param(operator, "view")?;
    require_known_view(strategy, &view)?;

    let mut value_index = BTreeMap::<String, BTreeSet<String>>::new();
    for observation in observations {
        if let Some(values) = observation.view_values.get(&view) {
            for value in values {
                value_index
                    .entry(value.clone())
                    .or_default()
                    .insert(observation.row_id.clone());
            }
        }
    }

    let operator_id = format!("exact_view:{view}");
    for row_ids in value_index.values() {
        add_pairs(row_ids, &operator_id, pair_hits);
    }

    Ok(())
}

fn apply_shared_anchor(
    strategy: &EntityStrategy,
    operator: &super::types::StrategyOperator,
    observations: &[PreparedObservation],
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) -> EntityResult<()> {
    let anchor = required_string_param(operator, "anchor")?;
    if !strategy.observations.anchor_fields.contains_key(&anchor) {
        return Err(strategy_error(
            "Blocking operator references an unknown anchor namespace",
            json!({ "anchor": anchor }),
        ));
    }

    let mut value_index = BTreeMap::<String, BTreeSet<String>>::new();
    for observation in observations {
        if let Some(values) = observation.anchors.get(&anchor) {
            for value in values {
                value_index
                    .entry(value.clone())
                    .or_default()
                    .insert(observation.row_id.clone());
            }
        }
    }

    let operator_id = format!("shared_anchor:{anchor}");
    for row_ids in value_index.values() {
        add_pairs(row_ids, &operator_id, pair_hits);
    }

    Ok(())
}

fn apply_registry_alias_match(
    _operator: &super::types::StrategyOperator,
    observations: &[PreparedObservation],
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) {
    let mut canonical_index = BTreeMap::<String, BTreeSet<String>>::new();
    for observation in observations {
        for canonical_id in &observation.registry_ids {
            canonical_index
                .entry(canonical_id.clone())
                .or_default()
                .insert(observation.row_id.clone());
        }
    }

    for row_ids in canonical_index.values() {
        add_pairs(row_ids, "registry_alias_match", pair_hits);
    }
}

fn apply_rare_token_overlap(
    strategy: &EntityStrategy,
    operator: &super::types::StrategyOperator,
    observations: &[PreparedObservation],
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) -> EntityResult<()> {
    let left_view = required_string_param(operator, "left_view")?;
    let right_view = required_string_param(operator, "right_view")?;
    let min_tokens = required_usize_param(operator, "min_tokens")?;
    let min_idf = required_f64_param(operator, "min_idf")?;

    require_known_view(strategy, &left_view)?;
    require_known_view(strategy, &right_view)?;

    if min_tokens == 0 || observations.len() < 2 {
        return Ok(());
    }

    let idf_by_token = compute_idf(observations, &left_view, &right_view);
    let operator_id = format!("rare_token_overlap:{left_view}:{right_view}");

    for left_index in 0..observations.len() {
        for right_index in left_index + 1..observations.len() {
            let left = &observations[left_index];
            let right = &observations[right_index];

            let overlaps = rare_overlap_count(
                left.view_tokens.get(&left_view),
                right.view_tokens.get(&right_view),
                &idf_by_token,
                min_idf,
            );

            let reverse_overlaps = if left_view == right_view {
                0
            } else {
                rare_overlap_count(
                    left.view_tokens.get(&right_view),
                    right.view_tokens.get(&left_view),
                    &idf_by_token,
                    min_idf,
                )
            };

            if overlaps >= min_tokens || reverse_overlaps >= min_tokens {
                insert_pair_hit(&left.row_id, &right.row_id, &operator_id, pair_hits);
            }
        }
    }

    Ok(())
}

fn compute_idf(
    observations: &[PreparedObservation],
    left_view: &str,
    right_view: &str,
) -> BTreeMap<String, f64> {
    let mut token_frequencies = BTreeMap::<String, usize>::new();

    for observation in observations {
        let mut tokens = BTreeSet::new();
        if let Some(view_tokens) = observation.view_tokens.get(left_view) {
            tokens.extend(view_tokens.iter().cloned());
        }
        if let Some(view_tokens) = observation.view_tokens.get(right_view) {
            tokens.extend(view_tokens.iter().cloned());
        }

        for token in tokens {
            *token_frequencies.entry(token).or_default() += 1;
        }
    }

    let total_observations = observations.len() as f64;
    token_frequencies
        .into_iter()
        .map(|(token, frequency)| (token, (total_observations / frequency as f64).ln()))
        .collect()
}

fn rare_overlap_count(
    left_tokens: Option<&BTreeSet<String>>,
    right_tokens: Option<&BTreeSet<String>>,
    idf_by_token: &BTreeMap<String, f64>,
    min_idf: f64,
) -> usize {
    match (left_tokens, right_tokens) {
        (Some(left), Some(right)) => left
            .intersection(right)
            .filter(|token| idf_by_token.get(*token).copied().unwrap_or_default() >= min_idf)
            .count(),
        _ => 0,
    }
}

fn add_pairs(
    row_ids: &BTreeSet<String>,
    operator_id: &str,
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) {
    let row_ids = row_ids.iter().cloned().collect::<Vec<_>>();
    for left_index in 0..row_ids.len() {
        for right_index in left_index + 1..row_ids.len() {
            insert_pair_hit(
                &row_ids[left_index],
                &row_ids[right_index],
                operator_id,
                pair_hits,
            );
        }
    }
}

fn insert_pair_hit(
    left_row_id: &str,
    right_row_id: &str,
    operator_id: &str,
    pair_hits: &mut BTreeMap<(String, String), BTreeSet<String>>,
) {
    if left_row_id == right_row_id {
        return;
    }

    let (left, right) = if left_row_id <= right_row_id {
        (left_row_id.to_string(), right_row_id.to_string())
    } else {
        (right_row_id.to_string(), left_row_id.to_string())
    };

    pair_hits
        .entry((left, right))
        .or_default()
        .insert(operator_id.to_string());
}

fn require_known_view(strategy: &EntityStrategy, view: &str) -> EntityResult<()> {
    if strategy.normalize.views.contains_key(view) {
        Ok(())
    } else {
        Err(strategy_error(
            "Blocking operator references an unknown normalized view",
            json!({ "view": view }),
        ))
    }
}

fn required_string_param(
    operator: &super::types::StrategyOperator,
    key: &str,
) -> EntityResult<String> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Blocking operator is missing a required string parameter",
            json!({ "operator": operator.op, "parameter": key }),
        )
    })?;

    value
        .as_str()
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            strategy_error(
                "Blocking operator parameter must be a non-empty string",
                json!({ "operator": operator.op, "parameter": key, "value": value }),
            )
        })
}

fn required_usize_param(
    operator: &super::types::StrategyOperator,
    key: &str,
) -> EntityResult<usize> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Blocking operator is missing a required integer parameter",
            json!({ "operator": operator.op, "parameter": key }),
        )
    })?;

    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            strategy_error(
                "Blocking operator parameter must be a non-negative integer",
                json!({ "operator": operator.op, "parameter": key, "value": value }),
            )
        })
}

fn required_f64_param(operator: &super::types::StrategyOperator, key: &str) -> EntityResult<f64> {
    let value = operator.params.get(key).ok_or_else(|| {
        strategy_error(
            "Blocking operator is missing a required numeric parameter",
            json!({ "operator": operator.op, "parameter": key }),
        )
    })?;

    value.as_f64().ok_or_else(|| {
        strategy_error(
            "Blocking operator parameter must be numeric",
            json!({ "operator": operator.op, "parameter": key, "value": value }),
        )
    })
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn strategy_error(message: &str, detail: Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::Strategy, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_runtime::types::{
        EntityStrategy, NormalizeConfig, ProjectedAnchor, ProjectedSurface, StrategyObservations,
        StrategyOperator,
    };

    #[test]
    fn exact_view_emits_block_with_provenance() {
        let strategy = test_strategy(vec![operator(
            "exact_view",
            &[("view", json!("core_name"))],
        )]);
        let incumbent = incumbent_memory();
        let observations = vec![
            observation("row-9", "Acme Corp. (1)"),
            observation("row-1", "ACME Corporation"),
        ];

        let records = build_candidate_blocks(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].left_row_id, "row-1");
        assert_eq!(records[0].right_row_id, "row-9");
        assert_eq!(records[0].strategy, strategy.reference());
        assert_eq!(records[0].registry_snapshot, incumbent.registry);
        assert_eq!(
            record_hit_ids(&records[0]),
            vec!["exact_view:core_name".to_string()]
        );
    }

    #[test]
    fn shared_anchor_emits_matching_pair() {
        let strategy = test_strategy(vec![operator("shared_anchor", &[("anchor", json!("lei"))])]);
        let incumbent = incumbent_memory();
        let observations = vec![
            observation_with_anchor("row-1", "Northwind", "lei", "549300AAA"),
            observation_with_anchor("row-3", "Northwind Holdings", "lei", "549300AAA"),
        ];

        let records = build_candidate_blocks(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(
            record_hit_ids(&records[0]),
            vec!["shared_anchor:lei".to_string()]
        );
    }

    #[test]
    fn registry_alias_match_uses_incumbent_alias_memory() {
        let strategy = test_strategy(vec![operator("registry_alias_match", &[])]);
        let mut incumbent = incumbent_memory();
        incumbent.alias_entries = vec![
            alias_entry("Acme Holdings", "IC-123"),
            alias_entry("ACME HLDGS", "IC-123"),
        ];

        let observations = vec![
            observation("row-2", "Acme Holdings"),
            observation("row-8", "ACME HLDGS"),
        ];

        let records = build_candidate_blocks(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(
            record_hit_ids(&records[0]),
            vec!["registry_alias_match".to_string()]
        );
    }

    #[test]
    fn rare_token_overlap_requires_enough_high_idf_overlap() {
        let strategy = test_strategy(vec![operator(
            "rare_token_overlap",
            &[
                ("left_view", json!("rare_tokens")),
                ("right_view", json!("rare_tokens")),
                ("min_tokens", json!(2)),
                ("min_idf", json!(4.0)),
            ],
        )]);
        let incumbent = incumbent_memory();

        let mut observations = vec![
            observation("row-a", "Alpha Zephyr Holdings"),
            observation("row-b", "Zephyr Alpha LP"),
        ];
        for index in 0..118 {
            observations.push(observation(
                &format!("row-filler-{index:03}"),
                &format!("Filler UniqueToken{index:03}"),
            ));
        }

        let records = build_candidate_blocks(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].left_row_id, "row-a");
        assert_eq!(records[0].right_row_id, "row-b");
        assert_eq!(
            record_hit_ids(&records[0]),
            vec!["rare_token_overlap:rare_tokens:rare_tokens".to_string()]
        );
    }

    #[test]
    fn block_hits_are_deduplicated_and_pair_order_is_canonical() {
        let strategy = test_strategy(vec![
            operator("exact_view", &[("view", json!("core_name"))]),
            operator("shared_anchor", &[("anchor", json!("lei"))]),
            operator("registry_alias_match", &[]),
        ]);
        let mut incumbent = incumbent_memory();
        incumbent.alias_entries = vec![
            alias_entry("Acme Corp.", "IC-777"),
            alias_entry("ACME Corporation", "IC-777"),
        ];

        let mut left = observation_with_anchor("row-9", "Acme Corp.", "lei", "549300AAA");
        left.alias_surfaces.push(ProjectedSurface {
            value: "Acme Corp.".to_string(),
            field: "alias_surfaces_json".to_string(),
        });
        let right = observation_with_anchor("row-1", "ACME Corporation", "lei", "549300AAA");

        let records = build_candidate_blocks(&strategy, &[left, right], &incumbent).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].left_row_id, "row-1");
        assert_eq!(records[0].right_row_id, "row-9");
        assert_eq!(
            record_hit_ids(&records[0]),
            vec![
                "exact_view:core_name".to_string(),
                "registry_alias_match".to_string(),
                "shared_anchor:lei".to_string(),
            ]
        );
    }

    #[test]
    fn candidate_pair_estimate_reports_dense_exact_bucket_before_expansion() {
        let strategy = test_strategy(vec![operator(
            "exact_view",
            &[("view", json!("core_name"))],
        )]);
        let incumbent = incumbent_memory();
        let observations = (0..1_001)
            .map(|index| observation(&format!("row-{index:04}"), "Computershare Trust Company"))
            .collect::<Vec<_>>();

        let estimate =
            estimate_candidate_block_pairs(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(estimate.total_pair_expansions, 500_500);
        assert_eq!(estimate.max_operator_pair_expansions, 500_500);
        assert_eq!(estimate.max_bucket_pair_expansions, 500_500);
        assert_eq!(estimate.max_bucket_row_count, 1_001);
        assert_eq!(estimate.max_bucket_operator_id, "exact_view:core_name");
        assert_eq!(estimate.max_bucket_value, "computershare trust");
    }

    #[test]
    fn candidate_pair_estimate_includes_registry_alias_buckets() {
        let strategy = test_strategy(vec![operator("registry_alias_match", &[])]);
        let mut incumbent = incumbent_memory();
        incumbent.alias_entries = vec![alias_entry("Trimont LLC", "ORG-001")];
        let observations = (0..4)
            .map(|index| observation(&format!("row-{index}"), "Trimont LLC"))
            .collect::<Vec<_>>();

        let estimate =
            estimate_candidate_block_pairs(&strategy, &observations, &incumbent).unwrap();

        assert_eq!(estimate.total_pair_expansions, 6);
        assert_eq!(estimate.max_bucket_pair_expansions, 6);
        assert_eq!(estimate.max_bucket_operator_id, "registry_alias_match");
        assert_eq!(estimate.max_bucket_value, "ORG-001");
    }

    fn test_strategy(blocking: Vec<StrategyOperator>) -> EntityStrategy {
        EntityStrategy {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:test".to_string(),
            observations: StrategyObservations {
                anchor_fields: BTreeMap::from([
                    ("lei".to_string(), "lei".to_string()),
                    ("figi".to_string(), "figi".to_string()),
                    ("cik".to_string(), "cik".to_string()),
                ]),
                ..StrategyObservations::default()
            },
            normalize: NormalizeConfig {
                views: BTreeMap::from([
                    (
                        "core_name".to_string(),
                        vec![
                            "lowercase".to_string(),
                            "strip_footnotes".to_string(),
                            "strip_legal_suffixes".to_string(),
                            "normalize_whitespace".to_string(),
                        ],
                    ),
                    (
                        "rare_tokens".to_string(),
                        vec!["tokenize".to_string(), "drop_stopwords".to_string()],
                    ),
                ]),
            },
            blocking,
            ..EntityStrategy::default()
        }
    }

    fn incumbent_memory() -> IncumbentMemory {
        IncumbentMemory {
            registry: crate::entity_runtime::types::RegistrySnapshot {
                id: "bdc-issuers".to_string(),
                version: "2026.03.01".to_string(),
                source: "registries/bdc-issuers".to_string(),
                lookup_snapshot_hash: "blake3:lookup".to_string(),
                escrow_snapshot_hash: "blake3:escrow".to_string(),
            },
            ..IncumbentMemory::default()
        }
    }

    fn observation(row_id: &str, name: &str) -> ProjectedObservation {
        ProjectedObservation {
            source_row_id: row_id.to_string(),
            doc_id: format!("doc-{row_id}"),
            primary_surface: ProjectedSurface {
                value: name.to_string(),
                field: "portfolio_company".to_string(),
            },
            ..ProjectedObservation::default()
        }
    }

    fn observation_with_anchor(
        row_id: &str,
        name: &str,
        namespace: &str,
        value: &str,
    ) -> ProjectedObservation {
        let mut observation = observation(row_id, name);
        observation.anchors.push(ProjectedAnchor {
            namespace: namespace.to_string(),
            value: value.to_string(),
            field: namespace.to_string(),
        });
        observation
    }

    fn alias_entry(input: &str, canonical_id: &str) -> super::super::types::AliasMappingEntry {
        super::super::types::AliasMappingEntry {
            input: input.to_string(),
            canonical_id: canonical_id.to_string(),
            canonical_type: "org_canon_id".to_string(),
            rule_id: "ORG_PROMOTION:bdc_org_graph.v1".to_string(),
        }
    }

    fn operator(name: &str, params: &[(&str, Value)]) -> StrategyOperator {
        StrategyOperator {
            op: name.to_string(),
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn record_hit_ids(record: &BlockRecord) -> Vec<String> {
        record
            .block_hits
            .iter()
            .map(|hit| hit.operator_id.clone())
            .collect()
    }
}
