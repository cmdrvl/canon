//! Person and trust names in `session derive` (bd-vdcw.26).
//!
//! Active only when the profile declares a normalized view with the
//! `person_name` operator. Then derive:
//! - classifies every observation (declared type wins; `unknown` gets the
//!   table-driven cues of `namekit::person`, and stays unknown when none apply),
//! - rejects fragments ("Jr", "LP") so they are never a node in evidence,
//! - compares people of declared type `person` by their parsed components
//!   instead of by a generic hit on the person view: equivalence is exact after
//!   listed steps, an initial against a full middle name is `name_similar`, and
//!   two different suffixes are `distinct`,
//! - relates a trust to the person it names (`related`, relation `trust_of`).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use crate::entity::profile::EntityProfileDocument;
use crate::namekit::person::{
    NameClass, PersonName, PersonRelation, classify_name, compare_persons, parse_person_name,
    trust_holder,
};

/// The operator that turns the mode on.
pub const PERSON_NAME_OPERATOR: &str = "person_name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FragmentRejection {
    pub obs_id: String,
    pub surface: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrustRelation {
    pub trust_obs_id: String,
    pub person_obs_id: String,
    pub relation: String,
    pub steps: Vec<String>,
}

/// What the name stage did, reported in the derive output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NameStageStats {
    /// Observations by classified type (declared type when the source knew it).
    pub classified: BTreeMap<String, usize>,
    /// Fragments rejected: never a node in evidence.
    pub fragments_rejected: Vec<FragmentRejection>,
    pub person_pairs_compared: usize,
    /// Trusts that name a person present in the session (`trust_of`). Reported,
    /// not written as evidence: session solve refuses evidence between two
    /// entity types, and a trust and a person are different kinds.
    pub trust_relations: Vec<TrustRelation>,
    /// Generic name hits (exact view, similarity) between two people that the
    /// person comparison replaced.
    pub generic_hits_deferred: usize,
}

/// The views of the profile that declare the `person_name` operator.
pub fn person_views(profile: &EntityProfileDocument) -> BTreeSet<String> {
    profile
        .normalized_views
        .iter()
        .filter(|(_, view)| view.operators.iter().any(|op| op == PERSON_NAME_OPERATOR))
        .map(|(name, _)| name.clone())
        .collect()
}

fn is_unstated(declared: &str) -> bool {
    matches!(
        declared.trim().to_ascii_lowercase().as_str(),
        "" | "unknown"
    )
}

/// The type a surface is compared under: the declared type when the source
/// knew it, else the classifier's org / trust, else unknown. Fragments are
/// handled by the caller.
pub fn effective_type(declared: &str, surface: &str) -> String {
    if !is_unstated(declared) {
        return declared.trim().to_ascii_lowercase();
    }
    match classify_name(surface) {
        NameClass::Org => "org".to_string(),
        NameClass::Trust => "trust".to_string(),
        NameClass::Person | NameClass::Fragment | NameClass::Unknown => declared.to_string(),
    }
}

/// Classify one observation for the summary counts. Returns the class and
/// whether it is a fragment.
pub fn classify_observation(declared: &str, surface: &str) -> (String, bool) {
    if classify_name(surface) == NameClass::Fragment {
        return ("fragment".to_string(), true);
    }
    if is_unstated(declared) {
        (classify_name(surface).as_str().to_string(), false)
    } else {
        (declared.trim().to_ascii_lowercase(), false)
    }
}

/// One side of a pair the name stage may relate.
pub struct NameNode<'a> {
    pub obs_id: &'a str,
    pub surface: &'a str,
    pub entity_type: &'a str,
    /// A committed registry alias, not a session observation. Two of these are
    /// never related to each other.
    pub registry: bool,
}

struct Person<'a> {
    node: &'a NameNode<'a>,
    parsed: PersonName,
}

fn base_basis(
    rule: &str,
    left: &str,
    right: &str,
    steps: &[String],
    explanation: &str,
) -> serde_json::Map<String, Value> {
    let mut basis = serde_json::Map::new();
    basis.insert("rule".to_string(), json!(rule));
    basis.insert("left_value".to_string(), json!(left));
    basis.insert("right_value".to_string(), json!(right));
    basis.insert("steps".to_string(), json!(steps));
    basis.insert("applies_to".to_string(), json!("surface"));
    basis.insert("explanation".to_string(), json!(explanation));
    basis
}

fn line(
    evidence_type: &str,
    left: &str,
    right: &str,
    basis: serde_json::Map<String, Value>,
    rule: &str,
    namespace: &str,
    profile_hash: &str,
) -> Value {
    json!({
        "type": evidence_type,
        "left": left,
        "right": right,
        "basis": Value::Object(basis),
        "source": {"channel": "canon", "id": format!("derive:{rule}")},
        "asserted_by": format!("canon:{namespace}:{rule}@{profile_hash}"),
    })
}

/// The assertion lines for the people and trusts among `nodes`, and how many
/// pairs were compared. `view_steps` are the person view's declared operators,
/// listed ahead of the person steps in every basis.
pub fn name_assertions(
    nodes: &[NameNode<'_>],
    view: &str,
    view_steps: &[String],
    namespace: &str,
    profile_hash: &str,
) -> (Vec<Value>, NameStageStats) {
    let rule = format!("{PERSON_NAME_OPERATOR}:{view}");
    let mut stats = NameStageStats::default();
    let mut lines = Vec::new();

    let people = nodes
        .iter()
        .filter(|node| node.entity_type == "person")
        .filter_map(|node| parse_person_name(node.surface).map(|parsed| Person { node, parsed }))
        .collect::<Vec<_>>();
    // Candidates share family and given name exactly; a family name plus an
    // initial is never a candidate.
    let mut groups = BTreeMap::<(String, String), Vec<&Person<'_>>>::new();
    for person in &people {
        groups
            .entry((person.parsed.family.clone(), person.parsed.given.clone()))
            .or_default()
            .push(person);
    }
    for members in groups.values() {
        for (index, left) in members.iter().enumerate() {
            for right in &members[index + 1..] {
                if left.node.obs_id == right.node.obs_id
                    || (left.node.registry && right.node.registry)
                {
                    continue;
                }
                stats.person_pairs_compared += 1;
                let (left, right) = if left.node.obs_id <= right.node.obs_id {
                    (left, right)
                } else {
                    (right, left)
                };
                let (left_key, right_key) = (left.parsed.key(), right.parsed.key());
                match compare_persons(&left.parsed, &right.parsed) {
                    PersonRelation::Equivalent { steps } => {
                        let all = view_steps.iter().cloned().chain(steps).collect::<Vec<_>>();
                        lines.push(line(
                            "name_equivalent",
                            left.node.obs_id,
                            right.node.obs_id,
                            base_basis(
                                &rule,
                                &left_key,
                                &right_key,
                                &all,
                                "same person after listed steps",
                            ),
                            &rule,
                            namespace,
                            profile_hash,
                        ));
                    }
                    PersonRelation::MiddleInitialVsName => lines.push(line(
                        "name_similar",
                        left.node.obs_id,
                        right.node.obs_id,
                        base_basis(
                            &rule,
                            &left_key,
                            &right_key,
                            &[],
                            "middle initial against a full middle name: review only",
                        ),
                        &rule,
                        namespace,
                        profile_hash,
                    )),
                    PersonRelation::SuffixConflict {
                        left: left_suffix,
                        right: right_suffix,
                    } => {
                        let mut basis = base_basis(
                            &rule,
                            &left_key,
                            &right_key,
                            &[],
                            &format!(
                                "generational suffixes differ: {left_suffix} and {right_suffix}"
                            ),
                        );
                        basis.insert(
                            "authority".to_string(),
                            json!("authoritative_incompatibility"),
                        );
                        lines.push(line(
                            "distinct",
                            left.node.obs_id,
                            right.node.obs_id,
                            basis,
                            &rule,
                            namespace,
                            profile_hash,
                        ));
                    }
                    PersonRelation::Unrelated => {}
                }
            }
        }
    }

    // A trust is related to the person it names, and never merged with them.
    let mut by_key = BTreeMap::<String, Vec<&Person<'_>>>::new();
    for person in &people {
        by_key.entry(person.parsed.key()).or_default().push(person);
    }
    for trust in nodes.iter().filter(|node| node.entity_type == "trust") {
        let Some(holder) = trust_holder(trust.surface).and_then(|text| parse_person_name(&text))
        else {
            continue;
        };
        for person in by_key
            .get(&holder.key())
            .into_iter()
            .flatten()
            .filter(|person| !(trust.registry && person.node.registry))
        {
            stats.trust_relations.push(TrustRelation {
                trust_obs_id: trust.obs_id.to_string(),
                person_obs_id: person.node.obs_id.to_string(),
                relation: "trust_of".to_string(),
                steps: view_steps
                    .iter()
                    .cloned()
                    .chain(holder.steps.iter().cloned())
                    .collect(),
            });
        }
    }
    (lines, stats)
}
