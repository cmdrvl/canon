#![forbid(unsafe_code)]

//! The session frontier: what the recorded solve still leaves unresolved, as
//! data, and the evidence kinds that would settle each part of it.
//!
//! Like explain, this is a projection over the committed session streams. It
//! re-uses the solve's own `missing` values and explain's observation reader;
//! it does not re-run tier arithmetic and it names evidence types and scopes,
//! never commands, queries or a ranking. `settles_with` is conditional on the
//! evidence existing: canon does not know that it does.

use crate::{
    Refusal,
    entity::{
        session::{SessionSolveArtifact, SessionSolveCluster},
        session_explain::{
            ObservationView, TIER_B_TYPES, explain_refusal, open_stream, read_observations,
            read_solve_hashed, stale_reasons,
        },
    },
    session::SESSION_OBSERVATIONS_STREAM_ID,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const SESSION_FRONTIER_VERSION: &str = "canon.entity.session.frontier.v1";

const NEEDS_AUTHORITATIVE_KEY: &str = "needs_authoritative_key";
const NEEDS_SECOND_EVIDENCE_TYPE: &str = "needs_second_evidence_type";
const NEEDS_INDEPENDENT_ORIGIN: &str = "needs_independent_origin";
const NEEDS_HUMAN: &str = "needs_human";
const UNKNOWN_SOURCE: &str = "<unknown>";

#[derive(Debug, Clone)]
pub struct SessionFrontierRequest {
    pub session: PathBuf,
    pub seed_entity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionFrontierOutput {
    pub version: String,
    pub solve_hash: String,
    pub coverage: Vec<FrontierCoverage>,
    pub unresolved_keys: Vec<FrontierUnresolvedKey>,
    pub components: Vec<FrontierComponent>,
    pub terminal: Vec<FrontierTerminal>,
    pub history_requests: Vec<FrontierHistoryRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<FrontierSeed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierCoverage {
    pub key_family: String,
    pub source: String,
    pub attempted: usize,
    pub supported: usize,
    pub conflicted: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierUnresolvedKey {
    pub namespace: String,
    pub value: String,
    pub surfaces: Vec<String>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierComponent {
    pub group_id: String,
    pub missing: String,
    pub settles_with: Vec<String>,
    pub items: usize,
    pub surfaces: Vec<String>,
    pub observation_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierTerminal {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierHistoryRequest {
    pub group_id: String,
    pub need: String,
    pub scope: BTreeMap<String, String>,
    pub before: Option<String>,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeed {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    pub neighbourhood: Vec<String>,
    pub unresolved_keys: Vec<FrontierSeedKey>,
    pub unresolved_spellings: Vec<FrontierSeedSpelling>,
    pub unresolved_units: Vec<FrontierSeedUnit>,
    pub history_requests: Vec<FrontierHistoryRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedKey {
    pub entity: String,
    pub namespace: String,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedSpelling {
    pub surface: String,
    pub group_id: String,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedUnit {
    pub surface: String,
    pub settles_with: Vec<String>,
}

impl SessionFrontierOutput {
    /// `session next`: the same data, one plain line per fact. Every number and
    /// name printed here is a field of the JSON.
    pub fn render_summary(&self) -> String {
        let mut lines = vec![format!(
            "frontier solve_hash={} groups={}",
            self.solve_hash,
            self.components.len()
        )];
        for group in &self.components {
            lines.push(format!(
                "group id={} items={} surfaces={} observations={} settles_with={}",
                group.group_id,
                group.items,
                group.surfaces.len(),
                group.observation_count,
                group.settles_with.join(",")
            ));
        }
        for row in &self.coverage {
            lines.push(format!(
                "coverage key_family={} source={} attempted={} supported={} conflicted={} unresolved={}",
                row.key_family,
                quoted(&row.source),
                row.attempted,
                row.supported,
                row.conflicted,
                row.unresolved
            ));
        }
        for key in &self.unresolved_keys {
            lines.push(format!(
                "unresolved_key namespace={} value={} surfaces={}",
                key.namespace,
                quoted(&key.value),
                key.surfaces.len()
            ));
        }
        for row in &self.terminal {
            lines.push(format!(
                "terminal reason={} count={}",
                row.reason, row.count
            ));
        }
        for request in &self.history_requests {
            lines.push(history_line("history", request));
        }
        if let Some(seed) = &self.seed {
            lines.push(format!(
                "seed query={} neighbourhood={} unresolved_keys={} unresolved_spellings={} unresolved_units={} history_requests={}",
                quoted(&seed.query),
                seed.neighbourhood.len(),
                seed.unresolved_keys.len(),
                seed.unresolved_spellings.len(),
                seed.unresolved_units.len(),
                seed.history_requests.len()
            ));
            for key in &seed.unresolved_keys {
                lines.push(format!(
                    "seed_key entity={} namespace={}",
                    quoted(&key.entity),
                    key.namespace
                ));
            }
            for unit in &seed.unresolved_units {
                lines.push(format!("seed_unit surface={}", quoted(&unit.surface)));
            }
        }
        lines.join("\n")
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn history_line(label: &str, request: &FrontierHistoryRequest) -> String {
    let scope = request
        .scope
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "{label} group={} need={} before={} scope={} settles_with={}",
        request.group_id,
        request.need,
        request.before.as_deref().unwrap_or("none"),
        quoted(&scope),
        request.settles_with.join(",")
    )
}

pub fn frontier(request: SessionFrontierRequest) -> Result<SessionFrontierOutput, Refusal> {
    let session = absolute(&request.session)?;
    let (solve, solve_hash) = read_solve_hashed(&session)?;
    let stale = stale_reasons(&session, &solve)?;
    if !stale.is_empty() {
        return Err(explain_refusal(
            "Session solve is stale; the frontier is bound to a current solve",
            json!({
                "reason": "stale_solve",
                "stale_reasons": stale,
                "writes_performed": false,
            }),
            "Run canon entity session solve, then rerun frontier",
        ));
    }
    let snapshot = open_stream(&session, SESSION_OBSERVATIONS_STREAM_ID)?;
    let observations = read_observations(&snapshot)?;
    build_frontier(
        &solve,
        &solve_hash,
        &observations,
        request.seed_entity.as_deref(),
    )
}

fn absolute(path: &Path) -> Result<PathBuf, Refusal> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| {
            explain_refusal(
                format!("Unable to resolve session path: {error}"),
                json!({"writes_performed": false}),
                "Pass an absolute session path",
            )
        })
}

/// One thing the frontier counts: a review pair, a contradiction or an
/// oversized cluster.
#[derive(Debug, Clone)]
struct Item {
    group_id: &'static str,
    surfaces: Vec<String>,
    settles_with: Vec<String>,
    /// Present for review pairs only.
    review: Option<ReviewFacts>,
}

#[derive(Debug, Clone)]
struct ReviewFacts {
    evidence_types: Vec<String>,
}

fn group_for_missing(missing: &str) -> &'static str {
    match missing {
        "second_lineage" => NEEDS_INDEPENDENT_ORIGIN,
        "second_evidence_type" => NEEDS_SECOND_EVIDENCE_TYPE,
        "required_evidence_type" => NEEDS_AUTHORITATIVE_KEY,
        _ => NEEDS_HUMAN,
    }
}

fn items_of(solve: &SessionSolveArtifact) -> Vec<Item> {
    let mut items = Vec::new();
    for review in &solve.review {
        let group_id = group_for_missing(&review.missing);
        let settles_with = match group_id {
            NEEDS_AUTHORITATIVE_KEY => solve.authoritative_keys.clone(),
            NEEDS_SECOND_EVIDENCE_TYPE => TIER_B_TYPES
                .iter()
                .filter(|kind| !review.evidence_types.iter().any(|seen| seen == **kind))
                .map(|kind| (*kind).to_string())
                .collect(),
            NEEDS_INDEPENDENT_ORIGIN => TIER_B_TYPES
                .iter()
                .map(|kind| (*kind).to_string())
                .collect(),
            _ => Vec::new(),
        };
        items.push(Item {
            group_id,
            surfaces: vec![review.left_surface.clone(), review.right_surface.clone()],
            settles_with,
            review: Some(ReviewFacts {
                evidence_types: review.evidence_types.clone(),
            }),
        });
    }
    for contradiction in &solve.contradictions {
        items.push(Item {
            group_id: NEEDS_HUMAN,
            surfaces: contradiction.surfaces.clone(),
            settles_with: Vec::new(),
            review: None,
        });
    }
    for cluster in solve
        .clusters
        .iter()
        .filter(|cluster| cluster.outcome == "oversized_cluster")
    {
        items.push(Item {
            group_id: NEEDS_HUMAN,
            surfaces: cluster.surfaces.clone(),
            settles_with: Vec::new(),
            review: None,
        });
    }
    items
}

pub(crate) fn build_frontier(
    solve: &SessionSolveArtifact,
    solve_hash: &str,
    observations: &[ObservationView],
    seed_entity: Option<&str>,
) -> Result<SessionFrontierOutput, Refusal> {
    let seed = match seed_entity {
        Some(query) => Some(resolve_seed(solve, observations, query)?),
        None => None,
    };
    let scope = seed.as_ref().map(|seed| &seed.neighbourhood);
    let in_scope = |surfaces: &[String]| {
        scope.is_none_or(|scope| surfaces.iter().any(|surface| scope.contains(surface)))
    };

    let by_surface = surfaces_to_observations(observations);
    let items = items_of(solve)
        .into_iter()
        .filter(|item| in_scope(&item.surfaces))
        .collect::<Vec<_>>();

    let components = components_of(&items, &by_surface);
    let history_requests = history_requests_of(&items, &by_surface);
    let coverage = coverage_of(solve, observations, scope);
    let unresolved_keys = unresolved_keys_of(solve, observations, scope);
    let terminal = terminal_of(solve, scope);

    let seed_block = seed.map(|seed| {
        seed_block_of(
            solve,
            observations,
            &items,
            &history_requests,
            seed,
            seed_entity.unwrap_or_default(),
        )
    });
    Ok(SessionFrontierOutput {
        version: SESSION_FRONTIER_VERSION.to_string(),
        solve_hash: solve_hash.to_string(),
        coverage,
        unresolved_keys,
        components,
        terminal,
        history_requests,
        seed: seed_block,
    })
}

fn surfaces_to_observations(
    observations: &[ObservationView],
) -> BTreeMap<&str, Vec<&ObservationView>> {
    let mut by_surface = BTreeMap::<&str, Vec<&ObservationView>>::new();
    for observation in observations {
        by_surface
            .entry(observation.surface.as_str())
            .or_default()
            .push(observation);
    }
    by_surface
}

fn components_of(
    items: &[Item],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierComponent> {
    let mut groups = BTreeMap::<&'static str, Vec<&Item>>::new();
    for item in items {
        groups.entry(item.group_id).or_default().push(item);
    }
    groups
        .into_iter()
        .map(|(group_id, group)| {
            let surfaces = group
                .iter()
                .flat_map(|item| item.surfaces.iter().cloned())
                .collect::<BTreeSet<_>>();
            let settles_with = group
                .iter()
                .flat_map(|item| item.settles_with.iter().cloned())
                .collect::<BTreeSet<_>>();
            let observation_count = surfaces
                .iter()
                .map(|surface| by_surface.get(surface.as_str()).map_or(0, Vec::len))
                .sum();
            FrontierComponent {
                group_id: group_id.to_string(),
                missing: group_id.to_string(),
                settles_with: settles_with.into_iter().collect(),
                items: group.len(),
                surfaces: surfaces.into_iter().collect(),
                observation_count,
            }
        })
        .collect()
}

/// A history request for each review pair that `same_slot` could settle: the
/// context the pair's observations share names the scope, and the earliest
/// as-of date among them is the `before` bound.
fn history_requests_of(
    items: &[Item],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierHistoryRequest> {
    let mut requests = BTreeSet::<(String, BTreeMap<String, String>, Option<String>)>::new();
    for item in items {
        let Some(review) = &item.review else {
            continue;
        };
        if review.evidence_types.iter().any(|kind| kind == "same_slot") {
            continue;
        }
        let observations = item
            .surfaces
            .iter()
            .flat_map(|surface| by_surface.get(surface.as_str()).into_iter().flatten())
            .copied()
            .collect::<Vec<_>>();
        let Some(scope) = shared_scope(&observations) else {
            continue;
        };
        let before = observations
            .iter()
            .filter_map(|observation| observation.as_of.clone())
            .min();
        requests.insert((item.group_id.to_string(), scope, before));
    }
    requests
        .into_iter()
        .map(|(group_id, scope, before)| FrontierHistoryRequest {
            group_id,
            need: "prior_observations".to_string(),
            scope,
            before,
            settles_with: vec!["same_slot".to_string()],
        })
        .collect()
}

/// Context entries every observation carries with the same value, leaving out
/// the as-of dates. `None` when nothing is shared.
fn shared_scope(observations: &[&ObservationView]) -> Option<BTreeMap<String, String>> {
    let mut iter = observations.iter();
    let first = iter.next()?;
    let mut scope = first
        .context
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "as_of" | "as_of_date"))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for observation in iter {
        scope.retain(|name, value| observation.context.get(name) == Some(value));
    }
    (!scope.is_empty()).then_some(scope)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SurfaceState {
    Supported,
    Conflicted,
    Unresolved,
}

fn surface_states(solve: &SessionSolveArtifact) -> BTreeMap<String, SurfaceState> {
    let mut states = BTreeMap::new();
    for cluster in &solve.clusters {
        let state = match cluster.outcome.as_str() {
            "accepted" | "resolved_existing" => SurfaceState::Supported,
            "contradiction" => SurfaceState::Conflicted,
            _ => SurfaceState::Unresolved,
        };
        for surface in &cluster.surfaces {
            states.insert(surface.clone(), state);
        }
    }
    for contradiction in &solve.contradictions {
        for surface in &contradiction.surfaces {
            states.insert(surface.clone(), SurfaceState::Conflicted);
        }
    }
    states
}

fn sources_of(observation: &ObservationView) -> Vec<String> {
    if observation.lineages.is_empty() {
        vec![UNKNOWN_SOURCE.to_string()]
    } else {
        observation.lineages.iter().cloned().collect()
    }
}

fn coverage_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierCoverage> {
    let states = surface_states(solve);
    let mut attempted = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for observation in observations {
        if scope.is_some_and(|scope| !scope.contains(&observation.surface)) {
            continue;
        }
        let families = observation
            .keys
            .keys()
            .cloned()
            .chain(std::iter::once("name".to_string()))
            .collect::<BTreeSet<_>>();
        for source in sources_of(observation) {
            for family in &families {
                attempted
                    .entry((family.clone(), source.clone()))
                    .or_default()
                    .insert(observation.surface.clone());
            }
        }
    }
    attempted
        .into_iter()
        .map(|((key_family, source), surfaces)| {
            let count = |wanted: SurfaceState| {
                surfaces
                    .iter()
                    .filter(|surface| {
                        states
                            .get(*surface)
                            .copied()
                            .unwrap_or(SurfaceState::Unresolved)
                            == wanted
                    })
                    .count()
            };
            FrontierCoverage {
                key_family,
                source,
                attempted: surfaces.len(),
                supported: count(SurfaceState::Supported),
                conflicted: count(SurfaceState::Conflicted),
                unresolved: count(SurfaceState::Unresolved),
            }
        })
        .collect()
}

fn has_partner(cluster: Option<&SessionSolveCluster>) -> bool {
    cluster.is_some_and(|cluster| {
        cluster.surfaces.len() > 1 || cluster.incumbent_canonical_id.is_some()
    })
}

fn unresolved_keys_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierUnresolvedKey> {
    let cluster_of = |surface: &str| {
        solve
            .clusters
            .iter()
            .find(|cluster| cluster.surfaces.iter().any(|member| member == surface))
    };
    let contradicted = solve
        .contradictions
        .iter()
        .flat_map(|contradiction| contradiction.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut values = BTreeMap::<(String, String), (BTreeSet<String>, BTreeSet<String>)>::new();
    for observation in observations {
        for namespace in &solve.authoritative_keys {
            for value in observation.keys.get(namespace).into_iter().flatten() {
                let entry = values
                    .entry((namespace.clone(), value.clone()))
                    .or_default();
                entry.0.insert(observation.surface.clone());
                entry.1.extend(sources_of(observation));
            }
        }
    }
    values
        .into_iter()
        .filter(|(_, (surfaces, _))| {
            let alone = surfaces.len() == 1
                && surfaces
                    .iter()
                    .all(|surface| !has_partner(cluster_of(surface)));
            let conflicting = surfaces
                .iter()
                .any(|surface| contradicted.contains(surface));
            (alone || conflicting)
                && scope.is_none_or(|scope| surfaces.iter().any(|surface| scope.contains(surface)))
        })
        .map(
            |((namespace, value), (surfaces, sources))| FrontierUnresolvedKey {
                namespace,
                value,
                surfaces: surfaces.into_iter().collect(),
                sources: sources.into_iter().collect(),
            },
        )
        .collect()
}

fn terminal_of(
    solve: &SessionSolveArtifact,
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierTerminal> {
    let in_review_or_contradiction = solve
        .review
        .iter()
        .flat_map(|review| [review.left_surface.clone(), review.right_surface.clone()])
        .chain(
            solve
                .contradictions
                .iter()
                .flat_map(|contradiction| contradiction.surfaces.iter().cloned()),
        )
        .collect::<BTreeSet<_>>();
    let standalone = solve
        .clusters
        .iter()
        .filter(|cluster| cluster.outcome == "standalone")
        .filter(|cluster| {
            cluster
                .surfaces
                .iter()
                .all(|surface| !in_review_or_contradiction.contains(surface))
        })
        .filter(|cluster| {
            scope.is_none_or(|scope| {
                cluster
                    .surfaces
                    .iter()
                    .any(|surface| scope.contains(surface))
            })
        })
        .count();
    let mut rows = Vec::new();
    if scope.is_none() && solve.dangling_evidence_count > 0 {
        rows.push(FrontierTerminal {
            reason: "dangling_evidence".to_string(),
            count: solve.dangling_evidence_count,
        });
    }
    if standalone > 0 {
        rows.push(FrontierTerminal {
            reason: "standalone_no_positive_evidence".to_string(),
            count: standalone,
        });
    }
    rows.sort_by(|left, right| left.reason.cmp(&right.reason));
    rows
}

struct ResolvedSeed {
    canonical_id: Option<String>,
    cluster_id: Option<String>,
    cluster_surfaces: BTreeSet<String>,
    linked: BTreeSet<String>,
    units: BTreeSet<String>,
    /// Surfaces linked by `part_of` evidence or detected as units.
    neighbourhood: BTreeSet<String>,
    part_of_linked: BTreeSet<String>,
}

fn resolve_seed(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    query: &str,
) -> Result<ResolvedSeed, Refusal> {
    let clusters = solve
        .clusters
        .iter()
        .filter(|cluster| {
            cluster.incumbent_canonical_id.as_deref() == Some(query)
                || cluster.surfaces.iter().any(|surface| surface == query)
        })
        .collect::<Vec<_>>();
    let mut cluster_surfaces = clusters
        .iter()
        .flat_map(|cluster| cluster.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    if cluster_surfaces.is_empty()
        && observations
            .iter()
            .any(|observation| observation.surface == query)
    {
        cluster_surfaces.insert(query.to_string());
    }
    if cluster_surfaces.is_empty() {
        return Err(explain_refusal(
            format!("Frontier seed entity '{query}' was not found"),
            json!({"seed_entity": query, "writes_performed": false}),
            "Run canon entity session status for the session state, then rerun frontier",
        ));
    }
    let mut linked = BTreeSet::new();
    for relation in solve.relations.iter().filter(|r| r.relation == "part_of") {
        let ends = [
            &relation.left_surface,
            &relation.right_surface,
            &relation.subject_surface,
            &relation.object_surface,
        ];
        if ends.iter().any(|end| cluster_surfaces.contains(*end)) {
            linked.extend(
                ends.iter()
                    .filter(|end| !end.is_empty() && !cluster_surfaces.contains(**end))
                    .map(|end| (*end).clone()),
            );
        }
    }
    let incumbent = clusters
        .iter()
        .find_map(|cluster| cluster.incumbent_canonical_id.clone());
    // A unit looks like one when its observations carry a role and either name
    // the seed in their context or start with the seed's surface.
    let mut units = BTreeSet::new();
    for observation in observations {
        if cluster_surfaces.contains(&observation.surface)
            || linked.contains(&observation.surface)
            || !observation.context.contains_key("role")
        {
            continue;
        }
        let names_seed = observation.context.values().any(|value| {
            cluster_surfaces.contains(value)
                || value == query
                || incumbent.as_deref() == Some(value.as_str())
        });
        let prefixed = cluster_surfaces.iter().any(|surface| {
            observation
                .surface
                .strip_prefix(surface.as_str())
                .is_some_and(|rest| rest.starts_with(' '))
        });
        if names_seed || prefixed {
            units.insert(observation.surface.clone());
        }
    }
    let neighbourhood = cluster_surfaces
        .iter()
        .chain(linked.iter())
        .chain(units.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    Ok(ResolvedSeed {
        canonical_id: incumbent,
        cluster_id: clusters.first().map(|cluster| cluster.cluster_id.clone()),
        cluster_surfaces,
        part_of_linked: linked.clone(),
        linked,
        units,
        neighbourhood,
    })
}

fn seed_block_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    items: &[Item],
    history_requests: &[FrontierHistoryRequest],
    seed: ResolvedSeed,
    query: &str,
) -> FrontierSeed {
    let keys_of = |surfaces: &BTreeSet<String>| {
        let mut present = BTreeSet::new();
        for observation in observations
            .iter()
            .filter(|observation| surfaces.contains(&observation.surface))
        {
            present.extend(observation.keys.keys().cloned());
        }
        for cluster in solve.clusters.iter().filter(|cluster| {
            cluster
                .surfaces
                .iter()
                .any(|surface| surfaces.contains(surface))
        }) {
            present.extend(cluster.keys.keys().cloned());
        }
        present
    };
    let mut entities = vec![(query.to_string(), seed.cluster_surfaces.clone())];
    for surface in seed.linked.iter().chain(seed.units.iter()) {
        entities.push((surface.clone(), BTreeSet::from([surface.clone()])));
    }
    let mut unresolved_keys = Vec::new();
    for (entity, surfaces) in &entities {
        let present = keys_of(surfaces);
        for namespace in &solve.authoritative_keys {
            if !present.contains(namespace) {
                unresolved_keys.push(FrontierSeedKey {
                    entity: entity.clone(),
                    namespace: namespace.clone(),
                    settles_with: vec!["same_key".to_string()],
                });
            }
        }
    }
    let mut spellings = BTreeMap::<String, (String, Vec<String>)>::new();
    for item in items.iter().filter(|item| item.review.is_some()) {
        for surface in &item.surfaces {
            spellings
                .entry(surface.clone())
                .or_insert_with(|| (item.group_id.to_string(), item.settles_with.clone()));
        }
    }
    let unresolved_spellings = spellings
        .into_iter()
        .map(|(surface, (group_id, settles_with))| FrontierSeedSpelling {
            surface,
            group_id,
            settles_with,
        })
        .collect();
    let unresolved_units = seed
        .units
        .iter()
        .filter(|surface| !seed.part_of_linked.contains(*surface))
        .map(|surface| FrontierSeedUnit {
            surface: surface.clone(),
            settles_with: vec!["part_of".to_string()],
        })
        .collect();
    FrontierSeed {
        query: query.to_string(),
        canonical_id: seed.canonical_id,
        cluster_id: seed.cluster_id,
        neighbourhood: seed.neighbourhood.into_iter().collect(),
        unresolved_keys,
        unresolved_spellings,
        unresolved_units,
        history_requests: history_requests.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::session::{
        SessionSolveContradiction, SessionSolveInputs, SessionSolveRegistry, SessionSolveReview,
    };

    fn solve(
        review: Vec<SessionSolveReview>,
        contradictions: Vec<SessionSolveContradiction>,
        clusters: Vec<SessionSolveCluster>,
    ) -> SessionSolveArtifact {
        SessionSolveArtifact {
            version: "v1".to_string(),
            inputs: SessionSolveInputs {
                observations_generation: "o".to_string(),
                evidence_generation: None,
                rules_hash: "r".to_string(),
                registry: SessionSolveRegistry {
                    id: "registry".to_string(),
                    version: "0.1.0".to_string(),
                    snapshot_hash: "s".to_string(),
                    escrow_snapshot_hash: String::new(),
                },
            },
            authoritative_keys: vec!["cik".to_string()],
            commit_key_namespaces: Vec::new(),
            summary: Default::default(),
            clusters,
            review,
            relations: Vec::new(),
            contradictions,
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
        }
    }

    fn review(left: &str, right: &str, types: &[&str], missing: &str) -> SessionSolveReview {
        SessionSolveReview {
            left_surface: left.to_string(),
            right_surface: right.to_string(),
            evidence_types: types.iter().map(|kind| (*kind).to_string()).collect(),
            lineage_count: 1,
            missing: missing.to_string(),
            known_pending: false,
            escrow_id: None,
        }
    }

    fn cluster(id: &str, outcome: &str, surfaces: &[&str]) -> SessionSolveCluster {
        SessionSolveCluster {
            cluster_id: id.to_string(),
            outcome: outcome.to_string(),
            tier: None,
            surfaces: surfaces
                .iter()
                .map(|surface| (*surface).to_string())
                .collect(),
            observation_count: surfaces.len(),
            keys: BTreeMap::new(),
            incumbent_canonical_id: None,
            accepting_evidence_ids: Vec::new(),
            reason: String::new(),
            conflict_reasons: Vec::new(),
        }
    }

    fn observation(id: &str, surface: &str, lineage: &str, cik: Option<&str>) -> ObservationView {
        ObservationView {
            obs_id: id.to_string(),
            surface: surface.to_string(),
            lineages: BTreeSet::from([lineage.to_string()]),
            keys: cik
                .map(|value| {
                    BTreeMap::from([("cik".to_string(), BTreeSet::from([value.to_string()]))])
                })
                .unwrap_or_default(),
            context: BTreeMap::new(),
            as_of: None,
        }
    }

    #[test]
    fn groups_split_by_missing_condition_with_their_settles_with() {
        let mut reviews = Vec::new();
        for index in 0..20 {
            reviews.push(review(
                &format!("o{index}a"),
                &format!("o{index}b"),
                &["name_equivalent", "same_slot"],
                "second_lineage",
            ));
        }
        for index in 0..7 {
            reviews.push(review(
                &format!("t{index}a"),
                &format!("t{index}b"),
                &["name_equivalent"],
                "second_evidence_type",
            ));
        }
        let contradictions = (0..3)
            .map(|index| SessionSolveContradiction {
                reason: "distinct".to_string(),
                surfaces: vec![format!("c{index}a"), format!("c{index}b")],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            })
            .collect();
        let artifact = solve(reviews, contradictions, Vec::new());
        let frontier = build_frontier(&artifact, "h", &[], None).unwrap();
        let groups = frontier
            .components
            .iter()
            .map(|group| (group.group_id.as_str(), group))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups["needs_independent_origin"].items, 20);
        assert_eq!(
            groups["needs_independent_origin"].settles_with,
            ["attribute_agreement", "name_equivalent", "same_slot"]
        );
        assert_eq!(groups["needs_second_evidence_type"].items, 7);
        assert_eq!(
            groups["needs_second_evidence_type"].settles_with,
            ["attribute_agreement", "same_slot"],
            "the type already present settles nothing new"
        );
        assert_eq!(groups["needs_human"].items, 3);
        assert!(groups["needs_human"].settles_with.is_empty());
        // Deterministic order: by group id.
        let ids = frontier
            .components
            .iter()
            .map(|group| group.group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "needs_human",
                "needs_independent_origin",
                "needs_second_evidence_type"
            ]
        );
    }

    #[test]
    fn required_type_gap_names_the_authoritative_namespaces() {
        let artifact = solve(
            vec![review(
                "a",
                "b",
                &["name_equivalent"],
                "required_evidence_type",
            )],
            Vec::new(),
            Vec::new(),
        );
        let frontier = build_frontier(&artifact, "h", &[], None).unwrap();
        assert_eq!(frontier.components[0].group_id, "needs_authoritative_key");
        assert_eq!(frontier.components[0].settles_with, ["cik"]);
    }

    #[test]
    fn coverage_counts_distinct_surfaces_per_family_and_source() {
        let artifact = solve(
            Vec::new(),
            vec![SessionSolveContradiction {
                reason: "key_conflict".to_string(),
                surfaces: vec!["Gamma".to_string()],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            }],
            vec![
                cluster("c1", "accepted", &["Alpha", "Alpha Inc"]),
                cluster("c2", "standalone", &["Beta"]),
                cluster("c3", "contradiction", &["Gamma"]),
            ],
        );
        let observations = vec![
            observation("1", "Alpha", "s1", Some("1")),
            observation("2", "Alpha", "s1", Some("1")),
            observation("3", "Alpha Inc", "s1", Some("1")),
            observation("4", "Beta", "s1", Some("2")),
            observation("5", "Gamma", "s1", Some("3")),
            observation("6", "Beta", "s2", None),
        ];
        let frontier = build_frontier(&artifact, "h", &observations, None).unwrap();
        let cik = frontier
            .coverage
            .iter()
            .find(|row| row.key_family == "cik" && row.source == "s1")
            .unwrap();
        assert_eq!(
            (cik.attempted, cik.supported, cik.conflicted, cik.unresolved),
            (4, 2, 1, 1),
            "two rows on Alpha are one surface"
        );
        let name = frontier
            .coverage
            .iter()
            .find(|row| row.key_family == "name" && row.source == "s2")
            .unwrap();
        assert_eq!((name.attempted, name.supported, name.unresolved), (1, 0, 1));
        // Beta's key sits on one surface with no partner; Gamma's is in a
        // contradiction; Alpha's key has partners and is not listed.
        let listed = frontier
            .unresolved_keys
            .iter()
            .map(|key| key.value.as_str())
            .collect::<Vec<_>>();
        assert_eq!(listed, ["2", "3"]);
    }

    #[test]
    fn history_request_takes_shared_context_and_the_earliest_date() {
        let artifact = solve(
            vec![review(
                "A",
                "B",
                &["name_equivalent"],
                "second_evidence_type",
            )],
            Vec::new(),
            Vec::new(),
        );
        let mut left = observation("1", "A", "s1", None);
        left.context = BTreeMap::from([
            ("deal".to_string(), "D".to_string()),
            ("loan".to_string(), "L".to_string()),
            ("role".to_string(), "sponsor".to_string()),
            ("as_of_date".to_string(), "2025-03-31".to_string()),
        ]);
        left.as_of = Some("2025-03-31".to_string());
        let mut right = observation("2", "B", "s2", None);
        right.context = BTreeMap::from([
            ("deal".to_string(), "D".to_string()),
            ("loan".to_string(), "L".to_string()),
            ("role".to_string(), "sponsor".to_string()),
            ("trustee".to_string(), "other".to_string()),
        ]);
        right.as_of = Some("2025-06-30".to_string());
        let frontier = build_frontier(&artifact, "h", &[left, right], None).unwrap();
        assert_eq!(frontier.history_requests.len(), 1);
        let request = &frontier.history_requests[0];
        assert_eq!(request.need, "prior_observations");
        assert_eq!(
            request.scope,
            BTreeMap::from([
                ("deal".to_string(), "D".to_string()),
                ("loan".to_string(), "L".to_string()),
                ("role".to_string(), "sponsor".to_string()),
            ])
        );
        assert_eq!(request.before.as_deref(), Some("2025-03-31"));
        assert_eq!(request.settles_with, ["same_slot"]);

        // A pair that already has same_slot needs no history.
        let artifact = solve(
            vec![review(
                "A",
                "B",
                &["same_slot", "name_equivalent"],
                "second_lineage",
            )],
            Vec::new(),
            Vec::new(),
        );
        assert!(
            build_frontier(&artifact, "h", &[], None)
                .unwrap()
                .history_requests
                .is_empty()
        );
    }

    #[test]
    fn no_field_is_named_how_command_or_recipe() {
        let artifact = solve(
            vec![review("A", "B", &["name_equivalent"], "second_lineage")],
            Vec::new(),
            vec![cluster("c", "standalone", &["A"])],
        );
        let frontier = build_frontier(&artifact, "h", &[], None).unwrap();
        let text = serde_json::to_string(&frontier).unwrap();
        for forbidden in ["\"how\"", "\"command\"", "\"recipe\"", "next_command"] {
            assert!(!text.contains(forbidden), "{forbidden} in {text}");
        }
    }
}
