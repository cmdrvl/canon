//! Alternates and member lists in `session derive` (bd-vdcw.24).
//!
//! Active only when the profile declares a normalized view with the
//! `name_structure` operator. Then derive parses every surface with
//! `namekit::structure` and turns only what the source string itself states
//! into evidence:
//! - a surface "P (dba Y)" is `asserted_alias` to an observation whose whole
//!   surface, after the listed steps, equals Y or P (the citation is the
//!   observation itself, with the marker text in the basis),
//! - a list "A and B" is `multi_party`: `related` (co_borrower_of) to the
//!   observations equal to a member, never `name_equivalent`; two lists are
//!   equivalent only when their member sets are equal.
//!
//! Nothing here is similarity, and no list member is guessed to be the "real"
//! entity.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use crate::entity::profile::EntityProfileDocument;
use crate::namekit::structure::{NameStructure, parse_name_structure, structure_key};
use crate::witness::hash_bytes;

/// The operator that turns the mode on.
pub const NAME_STRUCTURE_OPERATOR: &str = "name_structure";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MultiPartySurface {
    pub obs_id: String,
    pub members: Vec<String>,
}

/// Cap on the alternate surfaces listed in the derive output.
pub const ALTERNATE_SURFACES_LISTED: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlternateSurface {
    pub obs_id: String,
    pub surface: String,
    pub primary: String,
    pub alternates: Vec<String>,
}

/// What the structure stage did, reported in the derive output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StructureStats {
    pub surfaces_parsed: usize,
    /// Surfaces that state at least one alternate name.
    pub surfaces_with_alternates: usize,
    pub alternates: usize,
    pub alternates_by_marker: BTreeMap<String, usize>,
    /// The surfaces that state alternates, in observation order, up to
    /// `ALTERNATE_SURFACES_LISTED`.
    pub alternate_surfaces: Vec<AlternateSurface>,
    /// Surfaces parsed into a member list.
    pub multi_party: Vec<MultiPartySurface>,
    /// Surfaces where "and" / "&" was seen and not treated as a list.
    pub ambiguous_conjunctions: usize,
    pub asserted_alias_records: usize,
    pub co_borrower_records: usize,
    pub list_equivalence_records: usize,
    /// Pairs the structure stage skipped because the types differ.
    pub cross_type_skipped: usize,
    /// Generic name-equivalence hits that involved a list, which a list never
    /// has with a member.
    pub generic_hits_deferred: usize,
}

/// The views of the profile that declare the `name_structure` operator.
pub fn structure_views(profile: &EntityProfileDocument) -> BTreeSet<String> {
    profile
        .normalized_views
        .iter()
        .filter(|(_, view)| {
            view.operators
                .iter()
                .any(|op| op == NAME_STRUCTURE_OPERATOR)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// One observation the stage may relate.
pub struct StructureNode<'a> {
    pub obs_id: &'a str,
    pub surface: &'a str,
    pub entity_type: &'a str,
}

/// The observations that parse into a member list.
pub fn multi_party_ids(nodes: &[StructureNode<'_>]) -> BTreeSet<String> {
    nodes
        .iter()
        .filter(|node| parse_name_structure(node.surface).members.is_some())
        .map(|node| node.obs_id.to_string())
        .collect()
}

struct Parsed<'a> {
    node: &'a StructureNode<'a>,
    structure: NameStructure,
    key: String,
}

fn basis(
    rule: &str,
    left_value: &str,
    right_value: &str,
    steps: &[String],
    explanation: &str,
) -> serde_json::Map<String, Value> {
    let mut basis = serde_json::Map::new();
    basis.insert("rule".to_string(), json!(rule));
    basis.insert("left_value".to_string(), json!(left_value));
    basis.insert("right_value".to_string(), json!(right_value));
    basis.insert("steps".to_string(), json!(steps));
    basis.insert("applies_to".to_string(), json!("surface"));
    basis.insert("explanation".to_string(), json!(explanation));
    basis
}

fn ordered<'a>(first: &'a Parsed<'a>, second: &'a Parsed<'a>) -> (&'a Parsed<'a>, &'a Parsed<'a>) {
    if first.node.obs_id <= second.node.obs_id {
        (first, second)
    } else {
        (second, first)
    }
}

#[allow(clippy::too_many_arguments)]
fn line(
    evidence_type: &str,
    left: &str,
    right: &str,
    basis: serde_json::Map<String, Value>,
    rule: &str,
    namespace: &str,
    profile_hash: &str,
    source_extra: Option<(&str, &str)>,
) -> Value {
    let mut source = serde_json::Map::new();
    source.insert("channel".to_string(), json!("canon"));
    source.insert("id".to_string(), json!(format!("derive:{rule}")));
    if let Some((locator, content)) = source_extra {
        source.insert("locator".to_string(), json!(locator));
        source.insert(
            "content_hash".to_string(),
            json!(hash_bytes(content.as_bytes())),
        );
    }
    json!({
        "type": evidence_type,
        "left": left,
        "right": right,
        "basis": Value::Object(basis),
        "source": Value::Object(source),
        "asserted_by": format!("canon:{namespace}:{rule}@{profile_hash}"),
    })
}

/// The assertion lines for the alternates and lists among `nodes`.
pub fn structure_assertions(
    nodes: &[StructureNode<'_>],
    view: &str,
    namespace: &str,
    profile_hash: &str,
) -> (Vec<Value>, StructureStats) {
    let rule = format!("{NAME_STRUCTURE_OPERATOR}:{view}");
    let mut stats = StructureStats::default();
    let mut lines = Vec::new();

    let parsed = nodes
        .iter()
        .map(|node| {
            let structure = parse_name_structure(node.surface);
            let key = structure_key(node.surface).0;
            Parsed {
                node,
                structure,
                key,
            }
        })
        .collect::<Vec<_>>();
    stats.surfaces_parsed = parsed.len();
    // Whole-surface keys of surfaces that are one name (not a list).
    let mut by_key = BTreeMap::<String, Vec<&Parsed<'_>>>::new();
    for item in &parsed {
        if item.structure.members.is_none() {
            by_key.entry(item.key.clone()).or_default().push(item);
        }
    }

    for item in &parsed {
        if item
            .structure
            .steps
            .iter()
            .any(|step| step == "no_split:ambiguous_conjunction")
        {
            stats.ambiguous_conjunctions += 1;
        }
        if !item.structure.alternates.is_empty() {
            stats.surfaces_with_alternates += 1;
            stats.alternates += item.structure.alternates.len();
            if stats.alternate_surfaces.len() < ALTERNATE_SURFACES_LISTED {
                stats.alternate_surfaces.push(AlternateSurface {
                    obs_id: item.node.obs_id.to_string(),
                    surface: item.node.surface.to_string(),
                    primary: item.structure.primary.clone(),
                    alternates: item
                        .structure
                        .alternates
                        .iter()
                        .map(|alternate| format!("{}: {}", alternate.marker_text, alternate.name))
                        .collect(),
                });
            }
            for alternate in &item.structure.alternates {
                *stats
                    .alternates_by_marker
                    .entry(alternate.marker.as_str().to_string())
                    .or_default() += 1;
            }
        }
    }

    // Alternates: asserted_alias to a surface equal to the alternate or the primary.
    for item in &parsed {
        let primary = (!item.structure.primary.is_empty())
            .then_some(("primary", item.structure.primary.as_str()));
        for alternate in &item.structure.alternates {
            let targets = [("alternate", alternate.name.as_str())]
                .into_iter()
                .chain(primary);
            for (matched, text) in targets {
                let (key, form_steps) = structure_key(text);
                for other in by_key.get(&key).into_iter().flatten() {
                    if other.node.obs_id == item.node.obs_id {
                        continue;
                    }
                    if other.node.entity_type != item.node.entity_type {
                        stats.cross_type_skipped += 1;
                        continue;
                    }
                    let (left, right) = ordered(item, other);
                    let mut steps = item.structure.steps.clone();
                    steps.extend(form_steps.clone());
                    let mut line_basis = basis(
                        &rule,
                        &left.structure.primary,
                        &right.structure.primary,
                        &steps,
                        &format!(
                            "the source states the alias: {} {} {}",
                            item.structure.primary, alternate.marker_text, alternate.name
                        ),
                    );
                    line_basis.insert("marker".to_string(), json!(alternate.marker.as_str()));
                    line_basis.insert("marker_text".to_string(), json!(alternate.marker_text));
                    line_basis.insert("primary".to_string(), json!(item.structure.primary));
                    line_basis.insert("alternate".to_string(), json!(alternate.name));
                    line_basis.insert("matched".to_string(), json!(matched));
                    line_basis.insert("citing_observation".to_string(), json!(item.node.obs_id));
                    stats.asserted_alias_records += 1;
                    lines.push(line(
                        "asserted_alias",
                        left.node.obs_id,
                        right.node.obs_id,
                        line_basis,
                        &rule,
                        namespace,
                        profile_hash,
                        Some((
                            &format!("observation:{}", item.node.obs_id),
                            item.node.surface,
                        )),
                    ));
                }
            }
        }
    }

    // Lists: related to each member's own observation; equal only by member set.
    let mut member_sets = BTreeMap::<BTreeSet<String>, Vec<&Parsed<'_>>>::new();
    for item in &parsed {
        let Some(members) = item.structure.members.as_ref() else {
            continue;
        };
        stats.multi_party.push(MultiPartySurface {
            obs_id: item.node.obs_id.to_string(),
            members: members.clone(),
        });
        let mut member_keys = BTreeSet::new();
        for member in members {
            let (key, form_steps) = structure_key(member);
            member_keys.insert(key.clone());
            for other in by_key.get(&key).into_iter().flatten() {
                if other.node.obs_id == item.node.obs_id {
                    continue;
                }
                if other.node.entity_type != item.node.entity_type {
                    stats.cross_type_skipped += 1;
                    continue;
                }
                let (left, right) = ordered(item, other);
                let mut steps = item.structure.steps.clone();
                steps.extend(form_steps.clone());
                let mut line_basis = basis(
                    &rule,
                    &left.structure.primary,
                    &right.structure.primary,
                    &steps,
                    "one party of a list the source states; the list is not the member",
                );
                line_basis.insert("relation".to_string(), json!("co_borrower_of"));
                line_basis.insert("subject".to_string(), json!(item.node.obs_id));
                line_basis.insert("members".to_string(), json!(members));
                line_basis.insert("member".to_string(), json!(member));
                stats.co_borrower_records += 1;
                lines.push(line(
                    "related",
                    left.node.obs_id,
                    right.node.obs_id,
                    line_basis,
                    &rule,
                    namespace,
                    profile_hash,
                    None,
                ));
            }
        }
        if member_keys.len() >= 2 {
            member_sets.entry(member_keys).or_default().push(item);
        }
    }
    for group in member_sets.values() {
        for (index, first) in group.iter().enumerate() {
            for second in &group[index + 1..] {
                if first.node.entity_type != second.node.entity_type {
                    stats.cross_type_skipped += 1;
                    continue;
                }
                let (left, right) = ordered(first, second);
                let mut steps = vec!["name_structure:member_set".to_string()];
                steps.extend(first.structure.steps.clone());
                let mut line_basis = basis(
                    &rule,
                    &left.structure.primary,
                    &right.structure.primary,
                    &steps,
                    "the same set of parties after listed steps",
                );
                line_basis.insert(
                    "members".to_string(),
                    json!(left.structure.members.clone().unwrap_or_default()),
                );
                stats.list_equivalence_records += 1;
                lines.push(line(
                    "name_equivalent",
                    left.node.obs_id,
                    right.node.obs_id,
                    line_basis,
                    &rule,
                    namespace,
                    profile_hash,
                    None,
                ));
            }
        }
    }
    (lines, stats)
}
