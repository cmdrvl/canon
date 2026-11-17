//! `registry export --format link-evidence --from A --to B`: how are these two
//! linked, answered from the registry directory alone.
//!
//! The chain is the shortest path over committed outcomes only: alias and key
//! entries and superseded-id aliases are identity hops, `relations.jsonl` lines
//! are relation hops. No session directory is read, and nothing is inferred
//! from name similarity. A relation hop is never reported as identity.

use crate::{Refusal, RefusalCode};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

pub const REGISTRY_LINK_EVIDENCE_VERSION: &str = "canon_registry_link_evidence.v0";
const SUPERSEDED_ID_RULE: &str = "SUPERSEDED_ID_ALIAS";

#[derive(Debug, Clone)]
pub struct LinkEvidenceRequest {
    /// Registry directories in kind-name order (one for `--registry`).
    pub registries: Vec<PathBuf>,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkHop {
    pub from: String,
    pub to: String,
    /// `identity` or `relation`.
    pub hop: String,
    pub relation: Option<String>,
    pub outcome_ref: Option<String>,
    pub closure_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The registry the hop was read from, named when several are mapped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkEvidenceOutput {
    pub version: String,
    pub from: String,
    pub to: String,
    pub chain: Option<Vec<LinkHop>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Which of `from` / `to` matched nothing in any registry.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl LinkEvidenceOutput {
    pub fn render_summary(&self) -> String {
        match &self.chain {
            None => format!(
                "link-evidence {} -> {}: {}",
                self.from,
                self.to,
                self.reason.as_deref().unwrap_or("no_committed_chain")
            ),
            Some(chain) => {
                let mut lines = vec![format!(
                    "link-evidence {} -> {}: {} hop(s)",
                    self.from,
                    self.to,
                    chain.len()
                )];
                for hop in chain {
                    lines.push(format!(
                        "{} {} -> {} outcome_ref={} closure_hash={}",
                        hop.relation.as_deref().unwrap_or(hop.hop.as_str()),
                        hop.from,
                        hop.to,
                        hop.outcome_ref.as_deref().unwrap_or("null"),
                        hop.closure_hash.as_deref().unwrap_or("null")
                    ));
                }
                lines.join("\n")
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    input: String,
    canonical_id: String,
    rule_id: String,
    outcome_ref: Option<String>,
    closure_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct Relation {
    subject: String,
    object: String,
    relation: String,
    outcome_ref: Option<String>,
    closure_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct Loaded {
    id: String,
    entries: Vec<Entry>,
    relations: Vec<Relation>,
}

type Node = (usize, String);

#[derive(Debug, Clone)]
struct Edge {
    to: Node,
    hop: LinkHop,
}

fn refusal(message: impl Into<String>, detail: Value) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some("Point --registry or --registries at registry directories".to_string()),
    }
}

fn load(path: &Path) -> Result<Loaded, Refusal> {
    let display = path.display().to_string();
    let registry_json = fs::read(path.join("registry.json"))
        .map_err(|error| Refusal::bad_registry(&display, &format!("registry.json: {error}")))?;
    let registry_value: Value = serde_json::from_slice(&registry_json)
        .map_err(|error| Refusal::bad_registry(&display, &format!("registry.json: {error}")))?;
    let id = registry_value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Refusal::bad_registry(&display, "registry.json has no id"))?
        .to_string();

    let mut files = fs::read_dir(path)
        .map_err(|error| Refusal::bad_registry(&display, &error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.is_file()
                && candidate.extension() == Some("json".as_ref())
                && candidate.file_name() != Some("registry.json".as_ref())
                && candidate.file_name() != Some("_build.json".as_ref())
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut entries = Vec::new();
    for file in files {
        let bytes = fs::read(&file).map_err(|error| {
            Refusal::bad_registry(&display, &format!("{}: {error}", file.display()))
        })?;
        let Ok(Value::Array(rows)) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        for row in rows {
            let (Some(input), Some(canonical_id)) = (
                row.get("input").and_then(Value::as_str),
                row.get("canonical_id").and_then(Value::as_str),
            ) else {
                continue;
            };
            let authority = row.get("session_authority");
            entries.push(Entry {
                input: input.to_string(),
                canonical_id: canonical_id.to_string(),
                rule_id: row
                    .get("rule_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                outcome_ref: authority
                    .and_then(|value| value.get("outcome_ref"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                closure_hash: authority
                    .and_then(|value| value.get("closure_hash"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
    }

    let mut relations = Vec::new();
    if let Ok(text) = fs::read_to_string(path.join("relations.jsonl")) {
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(row) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let name = row
                .get("relation")
                .and_then(|relation| relation.get("value").or_else(|| relation.get("class")))
                .and_then(Value::as_str);
            let (Some(subject), Some(object), Some(name)) = (
                row.pointer("/subject/identity_id").and_then(Value::as_str),
                row.pointer("/object/identity_id").and_then(Value::as_str),
                name,
            ) else {
                continue;
            };
            let provenance = row.get("provenance");
            relations.push(Relation {
                subject: subject.to_string(),
                object: object.to_string(),
                relation: name.to_string(),
                outcome_ref: provenance
                    .and_then(|value| value.get("outcome_ref"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                closure_hash: provenance
                    .and_then(|value| value.get("closure_hash"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
    }
    Ok(Loaded {
        id,
        entries,
        relations,
    })
}

struct Graph {
    registries: Vec<Loaded>,
    multi: bool,
    owner: BTreeMap<String, usize>,
    adjacency: BTreeMap<Node, Vec<Edge>>,
}

impl Graph {
    fn label(&self, node: &Node) -> String {
        if self.multi {
            format!("{}:{}", self.registries[node.0].id, node.1)
        } else {
            node.1.clone()
        }
    }

    fn registry_name(&self, index: usize) -> Option<String> {
        self.multi.then(|| self.registries[index].id.clone())
    }

    fn identity_hop(&self, registry: usize, from: String, to: String, entry: &Entry) -> LinkHop {
        LinkHop {
            from,
            to,
            hop: "identity".to_string(),
            relation: None,
            outcome_ref: entry.outcome_ref.clone(),
            closure_hash: entry.closure_hash.clone(),
            reason: entry
                .closure_hash
                .is_none()
                .then(|| "no_session_authority".to_string()),
            registry: self.registry_name(registry),
        }
    }

    fn build(registries: Vec<Loaded>, multi: bool) -> Self {
        let mut owner = BTreeMap::new();
        for (index, registry) in registries.iter().enumerate() {
            for entry in &registry.entries {
                owner.entry(entry.canonical_id.clone()).or_insert(index);
            }
        }
        let mut graph = Self {
            registries,
            multi,
            owner,
            adjacency: BTreeMap::new(),
        };
        let mut edges = Vec::<(Node, Edge)>::new();
        for (index, registry) in graph.registries.iter().enumerate() {
            let has_id = |id: &str| {
                registry
                    .entries
                    .iter()
                    .any(|entry| entry.canonical_id == id)
            };
            let place = |id: &str| -> Node {
                if has_id(id) {
                    (index, id.to_string())
                } else {
                    (
                        graph.owner.get(id).copied().unwrap_or(index),
                        id.to_string(),
                    )
                }
            };
            for relation in &registry.relations {
                let subject = (index, relation.subject.clone());
                let object = place(&relation.object);
                let forward = LinkHop {
                    from: graph.label(&subject),
                    to: graph.label(&object),
                    hop: "relation".to_string(),
                    relation: Some(relation.relation.clone()),
                    outcome_ref: relation.outcome_ref.clone(),
                    closure_hash: relation.closure_hash.clone(),
                    reason: relation
                        .closure_hash
                        .is_none()
                        .then(|| "no_session_authority".to_string()),
                    registry: graph.registry_name(index),
                };
                let backward = LinkHop {
                    from: forward.to.clone(),
                    to: forward.from.clone(),
                    ..forward.clone()
                };
                edges.push((
                    subject.clone(),
                    Edge {
                        to: object.clone(),
                        hop: forward,
                    },
                ));
                edges.push((
                    object,
                    Edge {
                        to: subject,
                        hop: backward,
                    },
                ));
            }
            for entry in registry
                .entries
                .iter()
                .filter(|entry| entry.rule_id == SUPERSEDED_ID_RULE)
            {
                let old = (index, entry.input.clone());
                let survivor = (index, entry.canonical_id.clone());
                if old == survivor {
                    continue;
                }
                let forward =
                    graph.identity_hop(index, graph.label(&old), graph.label(&survivor), entry);
                let backward = LinkHop {
                    from: forward.to.clone(),
                    to: forward.from.clone(),
                    ..forward.clone()
                };
                edges.push((
                    old.clone(),
                    Edge {
                        to: survivor.clone(),
                        hop: forward,
                    },
                ));
                edges.push((
                    survivor,
                    Edge {
                        to: old,
                        hop: backward,
                    },
                ));
            }
        }
        for (from, edge) in edges {
            graph.adjacency.entry(from).or_default().push(edge);
        }
        for edges in graph.adjacency.values_mut() {
            edges.sort_by(|left, right| {
                left.to
                    .cmp(&right.to)
                    .then_with(|| left.hop.relation.cmp(&right.hop.relation))
            });
        }
        graph
    }

    /// An endpoint: an exact surface, `key:<namespace>:<value>` or a canonical
    /// id, looked up in every registry in order. Returns the nodes it names and
    /// the identity hop that leads from the text to each (`None` for an id).
    fn resolve(&self, text: &str, as_start: bool) -> Vec<(Node, Option<LinkHop>)> {
        let key_rule = text
            .strip_prefix("key:")
            .and_then(|rest| rest.split_once(':'))
            .map(|(namespace, value)| {
                (
                    format!("SESSION_KEY_{}", namespace.to_ascii_uppercase()),
                    value,
                )
            });
        let mut found = Vec::new();
        for (index, registry) in self.registries.iter().enumerate() {
            let matched = match &key_rule {
                Some((rule, value)) => registry
                    .entries
                    .iter()
                    .find(|entry| entry.rule_id == *rule && entry.input == *value),
                None => registry.entries.iter().find(|entry| entry.input == text),
            };
            if let Some(entry) = matched {
                let node = (index, entry.canonical_id.clone());
                let (from, to) = if as_start {
                    (text.to_string(), self.label(&node))
                } else {
                    (self.label(&node), text.to_string())
                };
                found.push((node, Some(self.identity_hop(index, from, to, entry))));
            } else if key_rule.is_none()
                && registry
                    .entries
                    .iter()
                    .any(|entry| entry.canonical_id == text)
            {
                found.push(((index, text.to_string()), None));
            }
        }
        found
    }

    /// Shortest chain from any start to any goal; neighbours expand in id
    /// order, so ties break by canonical id order.
    fn shortest(
        &self,
        starts: &[Node],
        goals: &BTreeSet<Node>,
    ) -> Option<(Node, Node, Vec<LinkHop>)> {
        let mut parent = BTreeMap::<Node, Option<(Node, LinkHop)>>::new();
        let mut queue = VecDeque::new();
        for start in starts {
            if parent.insert(start.clone(), None).is_none() {
                queue.push_back(start.clone());
            }
        }
        while let Some(node) = queue.pop_front() {
            if goals.contains(&node) {
                let goal = node.clone();
                let mut hops = Vec::new();
                let mut cursor = node;
                while let Some(Some((previous, hop))) = parent.get(&cursor) {
                    hops.push(hop.clone());
                    cursor = previous.clone();
                }
                hops.reverse();
                return Some((cursor, goal, hops));
            }
            for edge in self.adjacency.get(&node).into_iter().flatten() {
                if !parent.contains_key(&edge.to) {
                    parent.insert(edge.to.clone(), Some((node.clone(), edge.hop.clone())));
                    queue.push_back(edge.to.clone());
                }
            }
        }
        None
    }

    /// Relations that leave the registry (an object id owned by no mapped
    /// registry entry) are the sign that a wider `--registries` map may link.
    fn has_foreign_relation_endpoint(&self) -> bool {
        self.registries.iter().any(|registry| {
            registry.relations.iter().any(|relation| {
                !registry
                    .entries
                    .iter()
                    .any(|entry| entry.canonical_id == relation.object)
            })
        })
    }
}

pub fn link_evidence(request: LinkEvidenceRequest) -> Result<LinkEvidenceOutput, Refusal> {
    if request.registries.is_empty() {
        return Err(refusal(
            "link-evidence needs --registry or --registries",
            json!({"writes_performed": false}),
        ));
    }
    let multi = request.registries.len() > 1;
    let loaded = request
        .registries
        .iter()
        .map(|path| load(path))
        .collect::<Result<Vec<_>, _>>()?;
    let graph = Graph::build(loaded, multi);

    let starts = graph.resolve(&request.from, true);
    let ends = graph.resolve(&request.to, false);
    let mut unresolved = Vec::new();
    if starts.is_empty() {
        unresolved.push("from".to_string());
    }
    if ends.is_empty() {
        unresolved.push("to".to_string());
    }
    let none = |unresolved: Vec<String>, graph: &Graph| LinkEvidenceOutput {
        version: REGISTRY_LINK_EVIDENCE_VERSION.to_string(),
        from: request.from.clone(),
        to: request.to.clone(),
        chain: None,
        reason: Some("no_committed_chain".to_string()),
        unresolved,
        note: (!multi && graph.has_foreign_relation_endpoint()).then(|| {
            "a relation in this registry names an id outside it; rerun with --registries"
                .to_string()
        }),
    };
    if !unresolved.is_empty() {
        return Ok(none(unresolved, &graph));
    }

    let start_nodes = starts
        .iter()
        .map(|(node, _)| node.clone())
        .collect::<Vec<_>>();
    let goal_nodes = ends
        .iter()
        .map(|(node, _)| node.clone())
        .collect::<BTreeSet<_>>();
    let Some((start_node, end_node, middle)) = graph.shortest(&start_nodes, &goal_nodes) else {
        return Ok(none(Vec::new(), &graph));
    };
    let start = starts.iter().find(|(node, _)| *node == start_node);
    let end = ends.iter().find(|(node, _)| *node == end_node);

    let mut chain = Vec::new();
    if let Some((_, Some(hop))) = start {
        chain.push(hop.clone());
    }
    chain.extend(middle);
    if let Some((_, Some(hop))) = end {
        chain.push(hop.clone());
    }
    Ok(LinkEvidenceOutput {
        version: REGISTRY_LINK_EVIDENCE_VERSION.to_string(),
        from: request.from,
        to: request.to,
        chain: Some(chain),
        reason: None,
        unresolved: Vec::new(),
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(input: &str, id: &str, rule: &str, closure: Option<&str>) -> Entry {
        Entry {
            input: input.to_string(),
            canonical_id: id.to_string(),
            rule_id: rule.to_string(),
            outcome_ref: closure.map(|hash| format!("cmdrvl://outcome/{hash}")),
            closure_hash: closure.map(str::to_string),
        }
    }

    fn relation(subject: &str, object: &str, name: &str, closure: Option<&str>) -> Relation {
        Relation {
            subject: subject.to_string(),
            object: object.to_string(),
            relation: name.to_string(),
            outcome_ref: closure.map(|hash| format!("cmdrvl://outcome/{hash}")),
            closure_hash: closure.map(str::to_string),
        }
    }

    fn registry(id: &str, entries: Vec<Entry>, relations: Vec<Relation>) -> Loaded {
        Loaded {
            id: id.to_string(),
            entries,
            relations,
        }
    }

    fn chain(graph: &Graph, from: &str, to: &str) -> Option<Vec<LinkHop>> {
        let starts = graph.resolve(from, true);
        let ends = graph.resolve(to, false);
        let nodes = starts
            .iter()
            .map(|(node, _)| node.clone())
            .collect::<Vec<_>>();
        let goals = ends.iter().map(|(node, _)| node.clone()).collect();
        let (start_node, end_node, middle) = graph.shortest(&nodes, &goals)?;
        let mut all = Vec::new();
        if let Some((_, Some(hop))) = starts.iter().find(|(node, _)| *node == start_node) {
            all.push(hop.clone());
        }
        all.extend(middle);
        if let Some((_, Some(hop))) = ends.iter().find(|(node, _)| *node == end_node) {
            all.push(hop.clone());
        }
        Some(all)
    }

    #[test]
    fn identity_only_chain_between_two_aliases_of_one_id() {
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![
                    entry("Acme", "ORG-1", "SESSION_TIER_A", Some("h1")),
                    entry("ACME Corp", "ORG-1", "SESSION_TIER_A", Some("h2")),
                ],
                Vec::new(),
            )],
            false,
        );
        let hops = chain(&graph, "Acme", "ACME Corp").unwrap();
        assert_eq!(hops.len(), 2);
        assert!(hops.iter().all(|hop| hop.hop == "identity"));
        assert_eq!(hops[0].closure_hash.as_deref(), Some("h1"));
        assert_eq!(hops[1].closure_hash.as_deref(), Some("h2"));
    }

    #[test]
    fn a_relation_hop_is_labelled_relation_and_never_identity() {
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![
                    entry("Fund A", "ORG-1", "SESSION_TIER_A", Some("h1")),
                    entry("Firm", "ORG-2", "SESSION_TIER_A", Some("h2")),
                ],
                vec![relation("ORG-1", "ORG-2", "part_of", Some("hr"))],
            )],
            false,
        );
        let hops = chain(&graph, "Fund A", "Firm").unwrap();
        let middle = &hops[1];
        assert_eq!(middle.hop, "relation");
        assert_eq!(middle.relation.as_deref(), Some("part_of"));
        assert_eq!(middle.closure_hash.as_deref(), Some("hr"));
        assert_eq!(
            hops.iter().filter(|hop| hop.hop == "identity").count(),
            2,
            "only the two alias hops are identity"
        );
    }

    #[test]
    fn a_superseded_id_is_an_identity_hop_to_its_survivor() {
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![
                    entry("Survivor Inc", "ORG-1", "SESSION_TIER_A", Some("h1")),
                    entry("ORG-9", "ORG-1", SUPERSEDED_ID_RULE, Some("hs")),
                    entry("Third Co", "ORG-3", "SESSION_TIER_A", Some("h3")),
                ],
                vec![relation("ORG-9", "ORG-3", "part_of", Some("hr"))],
            )],
            false,
        );
        let hops = chain(&graph, "Survivor Inc", "ORG-3").unwrap();
        // Survivor Inc -> ORG-1 (alias), ORG-1 -> ORG-9 (superseded), ORG-9 -> ORG-3 (relation).
        assert_eq!(hops.len(), 3);
        assert_eq!(hops[1].hop, "identity");
        assert_eq!(hops[1].closure_hash.as_deref(), Some("hs"));
        assert_eq!(hops[2].hop, "relation");
    }

    #[test]
    fn no_chain_between_look_alikes() {
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![
                    entry("BlackRock, Inc.", "ORG-1", "SESSION_TIER_A", Some("h1")),
                    entry(
                        "Black Rock Coffee Bar",
                        "ORG-2",
                        "SESSION_TIER_A",
                        Some("h2"),
                    ),
                ],
                Vec::new(),
            )],
            false,
        );
        assert!(chain(&graph, "Black Rock Coffee Bar", "BlackRock, Inc.").is_none());
    }

    #[test]
    fn a_hop_that_predates_sessions_has_no_closure() {
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![entry("Old Name", "ORG-1", "MANUAL", None)],
                Vec::new(),
            )],
            false,
        );
        let hops = chain(&graph, "Old Name", "ORG-1").unwrap();
        assert_eq!(hops[0].closure_hash, None);
        assert_eq!(hops[0].reason.as_deref(), Some("no_session_authority"));
    }

    #[test]
    fn ties_break_by_canonical_id_order() {
        // Two equally short routes from ORG-1 to ORG-4: through ORG-2 or ORG-3.
        let graph = Graph::build(
            vec![registry(
                "orgs",
                vec![
                    entry("One", "ORG-1", "SESSION_TIER_A", Some("h")),
                    entry("Four", "ORG-4", "SESSION_TIER_A", Some("h")),
                ],
                vec![
                    relation("ORG-1", "ORG-3", "part_of", Some("a")),
                    relation("ORG-3", "ORG-4", "part_of", Some("b")),
                    relation("ORG-1", "ORG-2", "part_of", Some("c")),
                    relation("ORG-2", "ORG-4", "part_of", Some("d")),
                ],
            )],
            false,
        );
        let hops = chain(&graph, "One", "Four").unwrap();
        assert_eq!(hops[1].to, "ORG-2", "{hops:?}");
    }

    #[test]
    fn identity_never_crosses_registries_but_relations_do() {
        let graph = Graph::build(
            vec![
                registry(
                    "funds",
                    vec![entry("Fund A", "FND-1", "SESSION_TIER_A", Some("h1"))],
                    vec![relation("FND-1", "ORG-7", "managed_by", Some("hm"))],
                ),
                registry(
                    "orgs",
                    vec![
                        entry("Unit", "ORG-7", "SESSION_TIER_A", Some("h2")),
                        entry("Firm", "ORG-8", "SESSION_TIER_A", Some("h3")),
                    ],
                    vec![relation("ORG-7", "ORG-8", "part_of", Some("hp"))],
                ),
            ],
            true,
        );
        let hops = chain(&graph, "Fund A", "Firm").unwrap();
        assert_eq!(hops.len(), 4);
        assert_eq!(hops[1].relation.as_deref(), Some("managed_by"));
        assert_eq!(hops[1].registry.as_deref(), Some("funds"));
        assert_eq!(hops[2].relation.as_deref(), Some("part_of"));
        assert_eq!(hops[2].registry.as_deref(), Some("orgs"));
        assert_eq!(hops[2].from, "orgs:ORG-7");
    }
}
