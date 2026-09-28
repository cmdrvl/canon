//! Profile-declared retrieval. Retrieval keys never authorize an identity merge.
use super::*;
use crate::entity::{
    index::ngram_index::{EntityNgramBuildConfig, EntityNgramSurface},
    postings::{EntityPostingBuildConfig, EntityPostingSurface},
    prepare::PreparedSurfaceRecord,
    profile::{EntityProfileDocument, EntityProfileError},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocking {
    pub operators: Vec<Operator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operator {
    AliasPatchMatch {
        #[serde(default)]
        pairs: Vec<AliasPatchPair>,
    },
    /// Explicit equality authority, preserving the pre-existing core-view
    /// hyperedge contract. Ordinary exact_view is retrieval only.
    ExactIdentity {
        view: String,
    },
    ExactView {
        view: String,
    },
    ExactAnchor {
        field: String,
    },
    CompositeKey {
        views: Vec<String>,
    },
    NgramTopk {
        view: String,
        k: usize,
        candidate_cap: usize,
        #[serde(default)]
        score_floor_units: Option<u32>,
        #[serde(default)]
        max_ngram_df: Option<usize>,
        #[serde(default)]
        partition_by: Vec<String>,
    },
    RareTokenOverlap {
        view: String,
        #[serde(default)]
        token_views: Vec<String>,
        #[serde(default)]
        include_primary_surface: bool,
        k: usize,
        candidate_cap: usize,
        max_posting_size: usize,
        #[serde(default)]
        partition_by: Vec<String>,
    },
}

impl Operator {
    pub fn id(&self) -> String {
        match self {
            Self::AliasPatchMatch { .. } => "alias_patch_match".to_string(),
            Self::ExactIdentity { view } => format!("exact_identity:{view}"),
            Self::ExactView { view } => format!("exact_view:{view}"),
            Self::ExactAnchor { field } => format!("exact_anchor:{field}"),
            Self::CompositeKey { views } => format!("composite_key:{}", views.join("+")),
            Self::NgramTopk { view, .. } => format!("ngram_topk:{view}"),
            Self::RareTokenOverlap { view, .. } => format!("rare_token_overlap:{view}"),
        }
    }

    fn views(&self) -> Vec<&String> {
        match self {
            Self::AliasPatchMatch { .. } => vec![],
            Self::ExactIdentity { view } => vec![view],
            Self::ExactView { view } => vec![view],
            Self::ExactAnchor { .. } => vec![],
            Self::CompositeKey { views } => views.iter().collect(),
            Self::NgramTopk {
                view, partition_by, ..
            } => std::iter::once(view).chain(partition_by).collect(),
            Self::RareTokenOverlap {
                view,
                partition_by,
                token_views,
                ..
            } => std::iter::once(view)
                .chain(partition_by)
                .chain(token_views)
                .collect(),
        }
    }
}

impl Blocking {
    pub fn validate(&self, profile: &EntityProfileDocument) -> Result<(), EntityProfileError> {
        if profile.normalized_views.len() > 1
            && profile
                .prepare
                .as_ref()
                .and_then(|mapping| mapping.canonical_surface_normalized_view.as_ref())
                .is_none()
        {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Declared blocking with multiple views requires prepare.canonical_surface_normalized_view",
                json!({"field": "prepare.canonical_surface_normalized_view"}),
            ));
        }
        let mut ids = BTreeSet::new();
        let invalid = |operator: &Operator, reason: &str| {
            EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Invalid blocking operator",
                json!({"operator_id": operator.id(), "reason": reason}),
            )
        };
        for operator in &self.operators {
            if !ids.insert(operator.id()) {
                return Err(invalid(operator, "duplicate operator id"));
            }
            for view in operator.views() {
                if !profile.normalized_views.contains_key(view) {
                    return Err(invalid(operator, &format!("undeclared view: {view}")));
                }
            }
            match operator {
                Operator::AliasPatchMatch { pairs }
                    if pairs.iter().any(|pair| {
                        pair.patch_id.trim().is_empty()
                            || pair.left_surface_id.trim().is_empty()
                            || pair.right_surface_id.trim().is_empty()
                    }) =>
                {
                    return Err(invalid(
                        operator,
                        "alias pairs require patch and surface ids",
                    ));
                }
                Operator::ExactIdentity { view }
                    if !profile.evidence.support.iter().any(|support| {
                        support.op == "exact_view" && support.view.as_ref() == Some(view)
                    }) =>
                {
                    return Err(invalid(
                        operator,
                        "equality authority requires declared exact_view support",
                    ));
                }
                Operator::ExactAnchor { field }
                    if !profile
                        .prepare
                        .as_ref()
                        .is_some_and(|mapping| mapping.anchor_fields.contains_key(field)) =>
                {
                    return Err(invalid(operator, &format!("undeclared anchor: {field}")));
                }
                Operator::CompositeKey { views }
                    if views.is_empty()
                        || views.iter().collect::<BTreeSet<_>>().len() != views.len() =>
                {
                    return Err(invalid(
                        operator,
                        "composite views must be nonempty and unique",
                    ));
                }
                Operator::NgramTopk {
                    k,
                    candidate_cap,
                    max_ngram_df,
                    partition_by,
                    ..
                } if *k == 0
                    || *candidate_cap == 0
                    || *max_ngram_df == Some(0)
                    || partition_by.iter().collect::<BTreeSet<_>>().len() != partition_by.len() =>
                {
                    return Err(invalid(
                        operator,
                        "invalid limits or duplicate partition view",
                    ));
                }
                Operator::RareTokenOverlap {
                    k,
                    candidate_cap,
                    max_posting_size,
                    partition_by,
                    ..
                } if *k == 0
                    || *candidate_cap == 0
                    || *max_posting_size == 0
                    || partition_by.iter().collect::<BTreeSet<_>>().len() != partition_by.len() =>
                {
                    return Err(invalid(
                        operator,
                        "invalid limits or duplicate partition view",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Explicit declarations generate scored pair candidates, never the legacy
/// equality hyperedges. Enumeration is bounded by the same candidate budgets.
/// Keys are tuples, so delimiters inside values cannot create false collisions.
pub fn generate(
    profile: &EntityProfileDocument,
    surfaces: &[PreparedSurfaceRecord],
    config: BlockCandidateBudgetConfig,
) -> Result<BlockCandidateGenerationResult, Refusal> {
    let blocking = profile
        .blocking
        .as_ref()
        .expect("declared blocking required");
    blocking
        .validate(profile)
        .map_err(|error| error.to_refusal())?;
    let mut accumulator = BlockCandidateAccumulator::default();
    let mut budget = CandidateBudgetTracker::new(config, surfaces.len());
    let mut diagnostics = Vec::new();
    let mut sorted_surfaces = surfaces.iter().collect::<Vec<_>>();
    sorted_surfaces.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
    for operator in &blocking.operators {
        if let Operator::AliasPatchMatch { pairs } = operator {
            let index = EntityPostingIndex::build(
                &surfaces
                    .iter()
                    .map(|surface| EntityPostingSurface::new(&surface.surface_id))
                    .collect::<Vec<_>>(),
                EntityPostingBuildConfig::default(),
            )
            .map_err(posting_layout_refusal)?;
            diagnostics.push(apply_alias_patch_operator(
                &index,
                &AliasPatchMatchBlockOperator::new(operator.id(), pairs.clone()),
                &mut accumulator,
                &mut budget,
            )?);
            continue;
        }
        if matches!(operator, Operator::ExactIdentity { .. }) {
            continue;
        }
        let id = operator.id();
        let mut groups = BTreeMap::<Vec<String>, Vec<&PreparedSurfaceRecord>>::new();
        for surface in &sorted_surfaces {
            let keys = match operator {
                Operator::AliasPatchMatch { .. } => unreachable!("handled above"),
                Operator::ExactIdentity { .. } => {
                    unreachable!("handled as explicit equality authority")
                }
                Operator::ExactAnchor { field } => surface
                    .anchors
                    .iter()
                    .filter(|anchor| {
                        &anchor.namespace == field && !placeholder(profile, &anchor.value)
                    })
                    .map(|anchor| vec![anchor.value.clone()])
                    .collect::<BTreeSet<_>>(),
                Operator::ExactView { view } => key(profile, surface, std::slice::from_ref(view))
                    .into_iter()
                    .collect(),
                Operator::CompositeKey { views } => {
                    key(profile, surface, views).into_iter().collect()
                }
                Operator::NgramTopk { partition_by, .. }
                | Operator::RareTokenOverlap { partition_by, .. } => {
                    key(profile, surface, partition_by).into_iter().collect()
                }
            };
            for key in keys {
                groups.entry(key).or_default().push(surface);
            }
        }
        let mut diagnostic = OperatorDiagnosticAccumulator::new(&id);
        let mut seen_pairs = BTreeSet::new();
        for members in groups.into_values() {
            match operator {
                Operator::AliasPatchMatch { .. } => unreachable!("handled above"),
                Operator::ExactIdentity { .. } => {
                    unreachable!("handled as explicit equality authority")
                }
                Operator::ExactView { .. }
                | Operator::ExactAnchor { .. }
                | Operator::CompositeKey { .. } => {
                    for (left_index, left) in members.iter().enumerate() {
                        for right in members.iter().skip(left_index + 1) {
                            if !seen_pairs.insert((&left.surface_id, &right.surface_id)) {
                                continue;
                            }
                            // Check before materializing each new pair; never truncate.
                            budget.push(BlockCandidateBudgetObservation::new(
                                &left.surface_id,
                                &id,
                                1,
                                0,
                            ))?;
                            accumulator.add_hit(
                                &left.surface_id,
                                &right.surface_id,
                                BlockCandidateHit {
                                    operator_id: id.clone(),
                                    rank: None,
                                    score_units: 0,
                                },
                            );
                            diagnostic.record_topk_counts(1, 1, 1, 0);
                        }
                    }
                }
                Operator::NgramTopk {
                    view,
                    k,
                    candidate_cap,
                    score_floor_units,
                    max_ngram_df,
                    ..
                } => {
                    let inputs = members
                        .iter()
                        .map(|surface| {
                            EntityNgramSurface::new(
                                surface.surface_id.clone(),
                                text_value(surface, view),
                            )
                        })
                        .collect::<Vec<_>>();
                    let index = EntityNgramIndex::build(&inputs, EntityNgramBuildConfig::default())
                        .map_err(ngram_index_refusal)?;
                    let config = NgramTopKBlockOperator {
                        operator_id: id.clone(),
                        k: *k,
                        candidate_cap: *candidate_cap,
                        score_floor_units: *score_floor_units,
                    };
                    let result = apply_ngram_topk_operator(
                        &profile.profile,
                        &index,
                        &config,
                        &mut accumulator,
                        &mut budget,
                        *max_ngram_df,
                    )?;
                    add_diagnostic(&mut diagnostic, result);
                }
                Operator::RareTokenOverlap {
                    view,
                    token_views,
                    include_primary_surface,
                    k,
                    candidate_cap,
                    max_posting_size,
                    ..
                } => {
                    let inputs = members
                        .iter()
                        .map(|surface| {
                            let value = text_value(surface, view);
                            let mut tokens = BTreeSet::new();
                            let views = if token_views.is_empty() {
                                std::slice::from_ref(view)
                            } else {
                                token_views.as_slice()
                            };
                            for name in views {
                                tokens.extend(
                                    text_value(surface, name)
                                        .split_whitespace()
                                        .map(str::to_string),
                                );
                            }
                            if *include_primary_surface {
                                tokens.extend(
                                    surface
                                        .primary_surface
                                        .split_whitespace()
                                        .map(str::to_string),
                                );
                            }
                            EntityPostingSurface::new(surface.surface_id.clone())
                                .with_exact_view(view.clone(), value.clone())
                                .with_tokens(tokens)
                        })
                        .collect::<Vec<_>>();
                    let index =
                        EntityPostingIndex::build(&inputs, EntityPostingBuildConfig::default())
                            .map_err(posting_layout_refusal)?;
                    let config = RareTokenOverlapBlockOperator::new(&id, view)
                        .with_topk(*k, *candidate_cap)
                        .with_max_posting_size(*max_posting_size);
                    let result = apply_rare_token_overlap_operator(
                        &profile.profile,
                        &index,
                        &config,
                        &mut accumulator,
                        &mut budget,
                    )?;
                    add_diagnostic(&mut diagnostic, result);
                }
            }
        }
        diagnostics.push(diagnostic.finish());
    }
    let mut result = finish_candidates(accumulator, budget, diagnostics)?;
    result.diagnostics.configuration = Some(BlockConfiguration {
        blocking: profile.blocking.clone(),
        placeholder_values: effective_placeholder_values(profile),
    });
    Ok(result)
}

fn add_diagnostic(
    total: &mut OperatorDiagnosticAccumulator,
    part: BlockOperatorCandidateDiagnostics,
) {
    total.input_candidate_count += part.input_candidate_count;
    total.eligible_candidate_count += part.eligible_candidate_count;
    total.emitted_candidate_count += part.emitted_candidate_count;
    total.suppressed_candidate_count += part.suppressed_candidate_count;
    total.large_posting_suppressed_count += part.large_posting_suppressed_count;
}

pub(crate) fn key(
    profile: &EntityProfileDocument,
    surface: &PreparedSurfaceRecord,
    views: &[String],
) -> Option<Vec<String>> {
    views
        .iter()
        .map(|view| view_value(profile, surface, view))
        .collect()
}

// Display text is not an identity key. Keep the legacy similarity input bytes;
// placeholder exclusion applies to exact/composite/partition keys and anchors.
fn text_value(surface: &PreparedSurfaceRecord, view: &str) -> String {
    surface
        .normalized_views
        .get(view)
        .map(|value| value.value.clone())
        .unwrap_or_default()
}

fn view_value(
    profile: &EntityProfileDocument,
    surface: &PreparedSurfaceRecord,
    view: &str,
) -> Option<String> {
    surface
        .normalized_views
        .get(view)
        .map(|value| &value.value)
        .filter(|value| !placeholder(profile, value))
        .cloned()
}

pub(crate) fn placeholder(profile: &EntityProfileDocument, value: &str) -> bool {
    if value.trim().is_empty() {
        return true;
    }
    if let Some(mapping) = profile
        .prepare
        .as_ref()
        .filter(|mapping| !mapping.placeholder_values.is_empty())
    {
        mapping
            .placeholder_values
            .iter()
            .any(|placeholder| placeholder.eq_ignore_ascii_case(value.trim()))
    } else {
        DEFAULT_PLACEHOLDERS
            .iter()
            .any(|placeholder| placeholder.eq_ignore_ascii_case(value.trim()))
    }
}

pub const DEFAULT_PLACEHOLDERS: &[&str] = &[
    "0",
    "unknown",
    "vacant",
    "na",
    "n/a",
    "none",
    "placeholder:0",
];

pub fn effective_placeholder_values(profile: &EntityProfileDocument) -> Vec<String> {
    profile
        .prepare
        .as_ref()
        .filter(|mapping| !mapping.placeholder_values.is_empty())
        .map(|mapping| mapping.placeholder_values.clone())
        .unwrap_or_else(|| {
            DEFAULT_PLACEHOLDERS
                .iter()
                .map(|value| (*value).to_string())
                .collect()
        })
}
