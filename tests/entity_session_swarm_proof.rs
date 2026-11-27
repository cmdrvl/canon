//! bd-hzbp: the evidence-session loop end to end, with four concurrent workers,
//! over generated names and places. Every name and address is invented here.
//!
//! The generator knows which records are one entity. Canon is never told. A
//! worker sees only what `frontier claim` returns, and looks each claimed pair
//! up in an evidence table that stands in for retrieval. Some of that table is
//! deliberately wrong (a second corroboration for two different entities), and
//! the matching veto must still hold the pair apart.
//!
//! What a pass does not show: accuracy on real data. The evidence is synthetic.
//! A pass shows that the engine follows its rules: it merges what has enough
//! independent support, parks what does not, never merges across a veto, loses
//! no concurrent write, and replays by exact lookup.
//!
//! The default scale runs with the target. The larger scale is opt-in:
//! `cargo test --release --features integration --test entity_session_swarm_proof
//! -- --ignored --nocapture`

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Barrier, Mutex},
    time::Instant,
};
use tempfile::TempDir;

const WORKERS: usize = 4;
const HELD_OUT_PAIRS: usize = 100;
/// Digest of the held-out names pairs and their labels. A generator change that
/// moves the graded sample fails here; it must not move silently.
const HELD_OUT_DIGEST: &str =
    "blake3:aceac1d687be8a30cceb311b93537fe6fb7f11175a6a855643364d73786e4e37";

const HASH: &str = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const NAME_RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  reg_id: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [name_equivalent, attribute_agreement, same_slot]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
  split_shared_names: true
"#;

const PLACE_RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  apn: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias, same_apn]
tier_b:
  eligible_evidence_types: [address_canonical_match, point_in_parcel, footprint_overlap]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

const FIRST: [&str; 20] = [
    "Amber", "Birch", "Cobalt", "Dune", "Ember", "Fallow", "Garnet", "Hazel", "Indigo", "Juniper",
    "Kestrel", "Larch", "Marlin", "Nettle", "Ochre", "Pewter", "Quartz", "Russet", "Sable",
    "Teasel",
];
const SECOND: [&str; 20] = [
    "Harbor", "Ridge", "Meadow", "Summit", "Crossing", "Hollow", "Landing", "Orchard", "Quarry",
    "Terrace", "Valley", "Wharf", "Bluff", "Cove", "Glade", "Heath", "Inlet", "Knoll", "Moor",
    "Spring",
];
const THIRD: [&str; 10] = [
    "Holdings",
    "Partners",
    "Capital",
    "Group",
    "Ventures",
    "Associates",
    "Equities",
    "Properties",
    "Advisors",
    "Management",
];

/// One graded question: are these two spellings one entity?
#[derive(Clone)]
struct Pair {
    left: String,
    right: String,
    same: bool,
}

#[derive(Default)]
struct Slice {
    observations: Vec<Value>,
    /// The upstream grouping step: one weak clue per candidate pair. Without a
    /// clue a record is not a frontier item.
    blocking: Vec<Value>,
    /// What a worker finds when it investigates a pair of observation ids.
    retrievable: BTreeMap<(String, String), Vec<Value>>,
    /// Ground truth: spelling to true entity. Shared spellings are left out.
    truth: BTreeMap<String, String>,
    pairs: Vec<(usize, Pair)>,
    /// Observation pairs that carry a veto.
    vetoed: Vec<(String, String)>,
    /// Observation pairs with one clue and nothing more.
    unsettled: usize,
    /// A spelling two entities share, with each entity's key.
    shared: Vec<(String, String, String)>,
}

impl Slice {
    fn observe(
        &mut self,
        obs_id: &str,
        entity_type: &str,
        surface: &str,
        lineage: &str,
        keys: Value,
        true_entity: Option<&str>,
    ) {
        self.observations.push(json!({
            "obs_id": obs_id,
            "entity_type": entity_type,
            "surface": surface,
            "keys": keys,
            "source": {"channel": "synthetic", "id": lineage, "lineage": lineage}
        }));
        if let Some(entity) = true_entity {
            let previous = self.truth.insert(surface.to_string(), entity.to_string());
            assert!(previous.is_none(), "generator reused spelling {surface}");
        }
    }

    fn block(&mut self, kind: &str, left: &str, right: &str) {
        self.blocking
            .push(evidence(kind, left, right, "upstream-grouping"));
    }

    fn retrievable(&mut self, kind: &str, left: &str, right: &str, lineage: &str) {
        self.retrievable
            .entry((left.to_string(), right.to_string()))
            .or_default()
            .push(evidence(kind, left, right, lineage));
    }

    fn pair(&mut self, family: usize, left: &str, right: &str, same: bool) {
        self.pairs.push((
            family,
            Pair {
                left: left.to_string(),
                right: right.to_string(),
                same,
            },
        ));
    }
}

fn evidence(kind: &str, left: &str, right: &str, lineage: &str) -> Value {
    let basis = match kind {
        "address_canonical_match" => json!({"canonical_address": "synthetic", "normalizer": "gen"}),
        "footprint_overlap" => json!({"method": "synthetic", "overlap_ratio": 0.9}),
        "point_in_parcel" => json!({"method": "synthetic"}),
        "disjoint_parcels" => json!({"distance_m": 60, "threshold_m": 25}),
        "distinct" => json!({"authority": "reviewed", "rule": "synthetic series member"}),
        _ => json!({"rule": "synthetic"}),
    };
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": basis,
        "source": {
            "channel": "synthetic",
            "id": lineage,
            "lineage": lineage,
            "locator": format!("{lineage}#{left}:{right}"),
            "content_hash": HASH
        },
        "asserted_by": "agent:swarm-proof"
    })
}

fn names_slice(families: usize) -> Slice {
    let mut slice = Slice::default();
    for family in 0..families {
        assert!(family < FIRST.len() * SECOND.len() * THIRD.len());
        let base = format!(
            "{} {} {}",
            FIRST[family % 20],
            SECOND[(family / 20) % 20],
            THIRD[family / 400]
        );
        let id = |part: &str| format!("n{family:05}-{part}");
        let (a, b, c) = (id("a"), id("b"), id("c"));
        let entity = id("entity");
        let other = id("other");
        let key = json!({"reg_id": format!("REG-{family:05}")});
        match family % 6 {
            // An authoritative key ties three spellings: no worker needed.
            0 => {
                let first = format!("{base} LLC");
                let second = format!("{} L.L.C.", base.to_uppercase());
                let third = format!("{base}, Limited Liability Company");
                for (obs, surface, lineage) in [
                    (&a, &first, "ledger"),
                    (&b, &second, "filing"),
                    (&c, &third, "intake"),
                ] {
                    slice.observe(
                        obs,
                        "organisation",
                        surface,
                        lineage,
                        key.clone(),
                        Some(&entity),
                    );
                }
                slice.pair(family, &first, &third, true);
            }
            // Legal-suffix variants without a key: a second, independent clue
            // can be retrieved.
            1 => {
                let (first, second) = (format!("{base} LP"), format!("{base}, L.P."));
                slice.observe(
                    &a,
                    "organisation",
                    &first,
                    "ledger",
                    json!({}),
                    Some(&entity),
                );
                slice.observe(
                    &b,
                    "organisation",
                    &second,
                    "filing",
                    json!({}),
                    Some(&entity),
                );
                slice.block("name_equivalent", &a, &b);
                slice.retrievable("attribute_agreement", &a, &b, "address-records");
                slice.pair(family, &first, &second, true);
            }
            // One clue and nothing else. Half really are one entity, so parking
            // them costs recall.
            2 => {
                let same = (family / 6) % 2 == 0;
                let (first, second) = (format!("{base} Inc"), format!("{base} Incorporated"));
                let second_entity = if same { &entity } else { &other };
                slice.observe(
                    &a,
                    "organisation",
                    &first,
                    "ledger",
                    json!({}),
                    Some(&entity),
                );
                slice.observe(
                    &b,
                    "organisation",
                    &second,
                    "filing",
                    json!({}),
                    Some(second_entity),
                );
                slice.block("name_equivalent", &a, &b);
                slice.unsettled += 1;
                slice.pair(family, &first, &second, same);
            }
            // Fund series. A wrong second clue is retrievable beside the veto.
            3 => {
                let (first, second) = (format!("{base} Fund I"), format!("{base} Fund II"));
                slice.observe(
                    &a,
                    "organisation",
                    &first,
                    "ledger",
                    json!({}),
                    Some(&entity),
                );
                slice.observe(
                    &b,
                    "organisation",
                    &second,
                    "filing",
                    json!({}),
                    Some(&other),
                );
                slice.block("name_equivalent", &a, &b);
                slice.retrievable("attribute_agreement", &a, &b, "address-records");
                slice.retrievable("distinct", &a, &b, "offering-documents");
                slice.vetoed.push((a.clone(), b.clone()));
                slice.pair(family, &first, &second, false);
            }
            // A misspelling without a key joins a keyed record.
            4 => {
                let first = format!("{base} Trust");
                let second = format!("{} Trsut", base);
                slice.observe(
                    &a,
                    "organisation",
                    &first,
                    "ledger",
                    key.clone(),
                    Some(&entity),
                );
                slice.observe(
                    &b,
                    "organisation",
                    &second,
                    "intake",
                    json!({}),
                    Some(&entity),
                );
                slice.block("name_equivalent", &a, &b);
                slice.retrievable("attribute_agreement", &a, &b, "address-records");
                slice.pair(family, &first, &second, true);
            }
            // Homonyms: one spelling under two keys.
            _ => {
                let shared = format!("{base} Company");
                let (first, second) =
                    (format!("{base} Company North"), format!("{base} Co. South"));
                let (left_key, right_key) =
                    (format!("REG-{family:05}-N"), format!("REG-{family:05}-S"));
                for (obs, surface, reg_id, truth) in [
                    ("a", &shared, &left_key, None),
                    ("b", &first, &left_key, Some(&entity)),
                    ("c", &shared, &right_key, None),
                    ("d", &second, &right_key, Some(&other)),
                ] {
                    slice.observe(
                        &id(obs),
                        "organisation",
                        surface,
                        obs,
                        json!({"reg_id": reg_id}),
                        truth.map(String::as_str),
                    );
                }
                slice.shared.push((shared, left_key, right_key));
                slice.pair(family, &first, &second, false);
            }
        }
    }
    slice
}

fn places_slice(properties: usize) -> Slice {
    let mut slice = Slice::default();
    for property in 0..properties {
        let id = |part: &str| format!("p{property:04}-{part}");
        let (a, b) = (id("a"), id("b"));
        let entity = id("building");
        let other = id("neighbour");
        let street = format!(
            "{} {} Avenue",
            FIRST[property % 20],
            SECOND[property / 20 % 20]
        );
        let number = 100 + 2 * (property / 400);
        let first = format!("{number} {street}");
        let second = match property % 4 {
            2 => format!("{} {street}", number + 1),
            _ => format!(
                "{} {} Plaza {number}",
                FIRST[property % 20],
                SECOND[property / 20 % 20]
            ),
        };
        let same = property % 4 != 2;
        slice.observe(
            &a,
            "building",
            &first,
            "rent-roll",
            json!({}),
            Some(&entity),
        );
        slice.observe(
            &b,
            "building",
            &second,
            "appraisal",
            json!({}),
            Some(if same { &entity } else { &other }),
        );
        slice.block("address_canonical_match", &a, &b);
        match property % 4 {
            // One clue only.
            1 => slice.unsettled += 1,
            // A nearby, different building: overlapping footprints suggest a
            // merge and the parcel distance forbids it.
            2 => {
                slice.retrievable("footprint_overlap", &a, &b, "footprints");
                slice.retrievable("disjoint_parcels", &a, &b, "parcel-map");
                slice.vetoed.push((a.clone(), b.clone()));
            }
            _ => slice.retrievable("footprint_overlap", &a, &b, "footprints"),
        }
        slice.pair(property, &first, &second, same);

        // Several records of one parcel, tied by its identifier, and the
        // neighbouring parcel under another.
        let apn = format!("{property:04}-0001");
        let parcel = id("parcel");
        for (part, surface, lineage) in [
            ("c", format!("Parcel {apn}"), "county"),
            ("d", format!("APN {apn}"), "appraisal"),
            ("e", format!("Tax Lot {apn}"), "assessor"),
        ] {
            slice.observe(
                &id(part),
                "parcel",
                &surface,
                lineage,
                json!({"apn": apn}),
                Some(&parcel),
            );
        }
        slice.observe(
            &id("f"),
            "parcel",
            &format!("Parcel {property:04}-0002"),
            "county",
            json!({"apn": format!("{property:04}-0002")}),
            Some(&id("next-parcel")),
        );
    }
    slice
}

fn p(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

fn canon(args: &[&str], allowed: &[i32]) -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .expect("canon runs");
    assert!(
        output
            .status
            .code()
            .is_some_and(|code| allowed.contains(&code)),
        "args={args:?} status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn canon_json(args: &[&str]) -> Value {
    serde_json::from_slice(&canon(args, &[0]).stdout).expect("JSON stdout")
}

fn write_lines(path: &Path, rows: &[Value]) {
    let text: String = rows.iter().map(|row| format!("{row}\n")).collect();
    fs::write(path, text).expect("write rows");
}

struct Session {
    root: PathBuf,
    dir: PathBuf,
    rules: PathBuf,
    kinds: Vec<(&'static str, PathBuf)>,
}

impl Session {
    fn new(root: PathBuf, rules: &str, kinds: &[(&'static str, &str)]) -> Self {
        fs::create_dir(&root).expect("slice dir");
        let mut map = String::from("version: canon.session.registries.v1\nkinds:\n");
        let mut registries = Vec::new();
        for (kind, prefix) in kinds {
            let registry = root.join(kind);
            map.push_str(&format!(
                "  {kind}:\n    registry: {}\n    registry_id: {kind}s\n    prefix: {prefix}\n    zero_pad: 5\n    canonical_type: {kind}\n    alias_file: aliases.json\n    key_file: keys.json\n",
                registry.display()
            ));
            registries.push((*kind, registry));
        }
        let map_path = root.join("registries.yaml");
        fs::write(&map_path, map).expect("registries map");
        let rules_path = root.join("rules.yaml");
        fs::write(&rules_path, rules).expect("rules");
        let dir = root.join("session");
        canon(
            &[
                "entity",
                "session",
                "init",
                "--goal",
                "swarm proof",
                "--work-dir",
                p(&dir),
                "--registries",
                p(&map_path),
            ],
            &[0],
        );
        Self {
            root,
            dir,
            rules: rules_path,
            kinds: registries,
        }
    }

    fn assert_rows(&self, name: &str, rows: &[Value]) -> Value {
        let path = self.root.join(name);
        write_lines(&path, rows);
        let result = canon_json(&["entity", "session", "assert", p(&self.dir), p(&path)]);
        assert_eq!(result["rejected"], json!([]), "{result}");
        result
    }

    fn solve_args(&self) -> [&str; 6] {
        [
            "entity",
            "session",
            "solve",
            p(&self.dir),
            "--rules",
            p(&self.rules),
        ]
    }

    fn solve(&self) -> Value {
        canon_json(&self.solve_args())
    }

    fn commit(&self) -> Value {
        canon_json(&[
            "entity",
            "session",
            "commit",
            p(&self.dir),
            "--bump",
            "patch",
        ])
    }

    fn registry(&self, kind: &str) -> &Path {
        &self
            .kinds
            .iter()
            .find(|(name, _)| *name == kind)
            .expect("known kind")
            .1
    }

    fn escrow(&self, kind: &str, file: &str) -> Vec<Value> {
        fs::read_to_string(self.registry(kind).join("_escrow").join(file))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("escrow row"))
            .collect()
    }

    /// Exact lookup of many values at once: value to canonical id, if resolved.
    fn lookup(&self, kind: &str, values: &[&str]) -> BTreeMap<String, Option<String>> {
        let input = self.root.join(format!("lookup-{kind}.csv"));
        let mut text = String::from("name\n");
        for value in values {
            text.push_str(&format!("\"{value}\"\n"));
        }
        fs::write(&input, text).expect("lookup input");
        let output = canon(
            &[
                p(&input),
                "--registry",
                p(self.registry(kind)),
                "--column",
                "name",
                "--emit",
                "json",
                "--explicit",
                "--plain-json-values",
            ],
            &[0, 1],
        );
        let payload: Value = serde_json::from_slice(&output.stdout).expect("lookup JSON");
        let mut found: BTreeMap<String, Option<String>> = values
            .iter()
            .map(|value| (value.to_string(), None))
            .collect();
        for mapping in payload["mappings"].as_array().expect("mappings") {
            let value = mapping["input"].as_str().expect("input").to_string();
            let id = mapping["canonical_id"].as_str().map(str::to_string);
            assert!(found.insert(value, id).is_some(), "unexpected {mapping}");
        }
        found
    }
}

#[derive(Default)]
struct SwarmRecord {
    claimed: Vec<(usize, String)>,
    asserted: Vec<Value>,
    accepted: u64,
    stale_claims: u64,
    solve_conflicts: u64,
}

/// How often one claim may be refused as stale before the run counts as stuck.
const CLAIM_ATTEMPTS: usize = 40;

/// Claim a batch the way an agent must: another worker's evidence makes the
/// solve stale, and the refusal says to solve again and retry.
fn claim_batch(
    session: &Session,
    worker: &str,
    limit: usize,
    record: &Mutex<SwarmRecord>,
) -> Value {
    let limit = limit.to_string();
    let mut last = Value::Null;
    for _ in 0..CLAIM_ATTEMPTS {
        let output = canon(
            &[
                "entity",
                "session",
                "frontier",
                "claim",
                p(&session.dir),
                "--worker",
                worker,
                "--limit",
                &limit,
                "--ttl-secs",
                "3600",
                "--emit",
                "json",
            ],
            &[0, 2],
        );
        let result: Value = serde_json::from_slice(&output.stdout).expect("claim JSON");
        if output.status.code() == Some(0) {
            return result;
        }
        assert_eq!(
            result["refusal"]["detail"]["reason"], "stale_solve",
            "{result}"
        );
        record.lock().expect("record").stale_claims += 1;
        solve_among_writers(session, record);
        last = result;
    }
    panic!("{worker} never got a current solve: {last}");
}

/// Solve while others solve too: the loser of the publication race is told to
/// run the same command again.
fn solve_among_writers(session: &Session, record: &Mutex<SwarmRecord>) {
    let mut last = Value::Null;
    for _ in 0..CLAIM_ATTEMPTS {
        let output = canon(&session.solve_args(), &[0, 2]);
        if output.status.code() == Some(0) {
            return;
        }
        let result: Value = serde_json::from_slice(&output.stdout).expect("solve JSON");
        assert!(
            result["refusal"]["next_command"]
                .as_str()
                .is_some_and(|next| next.starts_with("Rerun the same")),
            "{result}"
        );
        record.lock().expect("record").solve_conflicts += 1;
        last = result;
    }
    panic!("solve never published: {last}");
}

/// Four workers claim and assert at the same time until nothing is unclaimed.
/// Each holds its first batch before any of them goes on, so all four really do
/// hold live leases together.
fn run_swarm(session: &Session, slice: &Slice, limit: usize) -> SwarmRecord {
    let record = Mutex::new(SwarmRecord::default());
    let all_hold_a_batch = Barrier::new(WORKERS);
    std::thread::scope(|scope| {
        for worker in 0..WORKERS {
            let (record, all_hold_a_batch) = (&record, &all_hold_a_batch);
            scope.spawn(move || {
                let name = format!("worker-{worker}");
                for round in 0.. {
                    let claim = claim_batch(session, &name, limit, record);
                    if round == 0 {
                        all_hold_a_batch.wait();
                    }
                    let items = claim["work_items"].as_array().expect("work items");
                    if items.is_empty() {
                        break;
                    }
                    let mut found = Vec::new();
                    for item in items {
                        assert_eq!(item["observations_truncated"], false, "{item}");
                        let ids: Vec<&str> = item["observations"]
                            .as_array()
                            .expect("observations")
                            .iter()
                            .map(|row| row["obs_id"].as_str().expect("obs_id"))
                            .collect();
                        for left in &ids {
                            for right in &ids {
                                let key = (left.to_string(), right.to_string());
                                if let Some(rows) = slice.retrievable.get(&key) {
                                    found.extend(rows.iter().cloned());
                                }
                            }
                        }
                    }
                    let accepted = if found.is_empty() {
                        0
                    } else {
                        session.assert_rows(&format!("{name}-{round}.jsonl"), &found)["accepted"]
                            .as_u64()
                            .expect("accepted")
                    };
                    let mut record = record.lock().expect("record");
                    for id in claim["frontier_ids"].as_array().expect("frontier ids") {
                        record
                            .claimed
                            .push((worker, id.as_str().expect("id").to_string()));
                    }
                    record.accepted += accepted;
                    record.asserted.extend(found);
                }
            });
        }
    });
    record.into_inner().expect("record")
}

/// Stages 1 to 3, with the checks that hold for any slice. Returns the session
/// after its second commit.
fn run_stages(session: &Session, slice: &Slice, label: &str) {
    let observations = session.root.join("observations.jsonl");
    write_lines(&observations, &slice.observations);
    let ingest = canon_json(&[
        "entity",
        "session",
        "ingest",
        p(&session.dir),
        p(&observations),
    ]);
    assert_eq!(ingest["rejected"], json!([]), "{ingest}");
    assert_eq!(
        ingest["accepted"].as_u64(),
        Some(slice.observations.len() as u64)
    );
    session.assert_rows("blocking.jsonl", &slice.blocking);

    // Stage 1: keys alone settle what they can. No worker has run.
    let first = session.solve();
    assert!(
        first["summary"]["accepted_tier_a"].as_u64() > Some(0),
        "{}",
        first["summary"]
    );
    assert_eq!(
        first["summary"]["accepted_tier_b"], 0,
        "{}",
        first["summary"]
    );
    session.commit();
    session.solve();

    // Stage 2: the swarm works the residual frontier.
    let frontier = canon_json(&[
        "entity",
        "session",
        "frontier",
        p(&session.dir),
        "--emit",
        "json",
    ]);
    let open: BTreeSet<String> = frontier["work_items"]
        .as_array()
        .expect("work items")
        .iter()
        .map(|item| item["frontier_id"].as_str().expect("id").to_string())
        .collect();
    assert!(open.len() >= WORKERS * 2, "{} items", open.len());

    // About two batches a worker, so claims overlap with other workers' writes.
    let record = run_swarm(session, slice, open.len().div_ceil(WORKERS * 2));
    let claimed: BTreeSet<String> = record.claimed.iter().map(|(_, id)| id.clone()).collect();
    assert_eq!(
        claimed.len(),
        record.claimed.len(),
        "an item was claimed twice"
    );
    assert!(
        claimed.is_superset(&open),
        "the swarm left frontier items unclaimed"
    );
    let busy: BTreeSet<usize> = record.claimed.iter().map(|(worker, _)| *worker).collect();
    assert_eq!(busy.len(), WORKERS);

    // No lost writes: every retrievable row reached the session, each counted
    // as new exactly once however many workers sent it.
    let expected: usize = slice.retrievable.values().map(Vec::len).sum();
    let sent: BTreeSet<String> = record.asserted.iter().map(Value::to_string).collect();
    assert_eq!(sent.len(), expected);
    assert_eq!(record.accepted, expected as u64);
    let rows: Vec<Value> = slice.retrievable.values().flatten().cloned().collect();
    let replayed = session.assert_rows("replayed.jsonl", &rows);
    assert_eq!(replayed["accepted"], 0, "{replayed}");
    assert_eq!(replayed["already_present"].as_u64(), Some(expected as u64));
    eprintln!(
        "[{label}] swarm: {} items at the start, {} claimed, {} rows sent for {expected} unique, \
         {} claims refused as stale, {} solves retried",
        open.len(),
        claimed.len(),
        record.asserted.len(),
        record.stale_claims,
        record.solve_conflicts
    );

    // Stage 3: adjudication, vetoes, one commit.
    let solved = session.solve();
    eprintln!("[{label}] solve summary: {}", solved["summary"]);
    assert!(
        solved["summary"]["accepted_tier_b"].as_u64() > Some(0),
        "{}",
        solved["summary"]
    );
    session.commit();
    for (kind, registry) in &session.kinds {
        let lint = canon_json(&["registry", "lint", p(registry), "--emit", "json"]);
        eprintln!("[{label}] {kind} registry lint: {}", lint["outcome"]);
    }
}

/// Stage 4 for one kind: exports, exact replay, and no id spanning two true
/// entities. Returns the replayed ids.
fn export_and_replay(
    session: &Session,
    slice: &Slice,
    kind: &str,
    spellings: &[&str],
) -> BTreeMap<String, Option<String>> {
    let resolved = session.lookup(kind, spellings);
    let mut members: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (spelling, id) in &resolved {
        if let Some(id) = id {
            members
                .entry(id.as_str())
                .or_default()
                .insert(slice.truth[spelling].as_str());
        }
    }
    let false_merges: Vec<_> = members
        .iter()
        .filter(|(_, truth)| truth.len() > 1)
        .collect();
    assert!(false_merges.is_empty(), "false merges: {false_merges:?}");
    assert_eq!(session.lookup(kind, spellings), resolved, "replay differs");
    let keyed: BTreeSet<&str> = slice
        .observations
        .iter()
        .filter(|row| row["keys"].as_object().is_some_and(|keys| !keys.is_empty()))
        .filter_map(|row| row["surface"].as_str())
        .collect();

    for (format, extra) in [
        ("dbt-seed", &["--namespace", "synthetic"][..]),
        ("key-crosswalk", &[][..]),
    ] {
        let out = session.root.join(format!("{kind}-{format}.csv"));
        let mut args = vec![
            "registry",
            "export",
            "--registry",
            p(session.registry(kind)),
            "--format",
            format,
            "--out",
            p(&out),
        ];
        args.extend_from_slice(extra);
        canon(&args, &[0]);
        let table = fs::read_to_string(&out).expect("export table");
        let cells: BTreeSet<&str> = table
            .lines()
            .flat_map(|line| line.split(','))
            .map(|cell| cell.trim_matches('"'))
            .collect();
        // The seed lists every committed id. The crosswalk joins keys, so it
        // owes a row only to an id that has one.
        let owed: BTreeSet<&str> = resolved
            .iter()
            .filter(|(spelling, _)| format == "dbt-seed" || keyed.contains(spelling.as_str()))
            .filter_map(|(_, id)| id.as_deref())
            .collect();
        // Only buildings carry no key.
        assert_eq!(
            owed.is_empty(),
            kind == "building" && format == "key-crosswalk",
            "{kind} {format}"
        );
        for id in owed {
            assert!(cells.contains(id), "{format} export lacks {id}");
        }
    }
    resolved
}

/// Two families in five, spread over all six kinds.
fn in_held_out_split(family: usize) -> bool {
    matches!(family % 5, 0 | 3)
}

fn held_out(slice: &Slice) -> Vec<Pair> {
    let sample: Vec<Pair> = slice
        .pairs
        .iter()
        .filter(|(family, _)| in_held_out_split(*family))
        .take(HELD_OUT_PAIRS)
        .map(|(_, pair)| pair.clone())
        .collect();
    assert_eq!(sample.len(), HELD_OUT_PAIRS);
    sample
}

fn prove_names(root: &Path, families: usize) {
    let started = Instant::now();
    let slice = names_slice(families);
    let session = Session::new(root.join("names"), NAME_RULES, &[("organisation", "ORG")]);
    run_stages(&session, &slice, "names");

    let pending = session.escrow("organisation", "pending.jsonl");
    assert!(
        pending.len() >= slice.unsettled,
        "{} pending rows for {} unsettled pairs",
        pending.len(),
        slice.unsettled
    );
    assert_eq!(
        session.escrow("organisation", "cannot_link.jsonl").len(),
        slice.vetoed.len()
    );

    let spellings: Vec<&str> = slice.truth.keys().map(String::as_str).collect();
    let resolved = export_and_replay(&session, &slice, "organisation", &spellings);

    // A shared spelling belongs to neither entity; each key resolves alone.
    let asked: Vec<&str> = slice
        .shared
        .iter()
        .flat_map(|(shared, left, right)| [shared.as_str(), left, right])
        .collect();
    assert!(!asked.is_empty());
    let found = session.lookup("organisation", &asked);
    for (shared, left_key, right_key) in &slice.shared {
        assert_eq!(found[shared], None, "{shared}");
        assert!(found[left_key].is_some() && found[right_key].is_some());
        assert_ne!(found[left_key], found[right_key]);
    }

    let sample = held_out(&slice);
    let labels: String = sample
        .iter()
        .map(|pair| format!("{}\t{}\t{}\n", pair.left, pair.right, pair.same))
        .collect();
    let digest = format!("blake3:{}", blake3::hash(labels.as_bytes()).to_hex());
    assert_eq!(digest, HELD_OUT_DIGEST, "the held-out sample moved");

    let (mut true_positive, mut false_positive, mut false_negative, mut true_negative) =
        (0, 0, 0, 0);
    for pair in &sample {
        let (left, right) = (&resolved[&pair.left], &resolved[&pair.right]);
        match (left.is_some() && left == right, pair.same) {
            (true, true) => true_positive += 1,
            (true, false) => false_positive += 1,
            (false, true) => false_negative += 1,
            (false, false) => true_negative += 1,
        }
    }
    eprintln!(
        "[names] {} observations, {} spellings; held-out {HELD_OUT_PAIRS} pairs: \
         tp={true_positive} fp={false_positive} fn={false_negative} tn={true_negative} \
         precision={:.3} recall={:.3}; {:.1?}",
        slice.observations.len(),
        spellings.len(),
        f64::from(true_positive) / f64::from(true_positive + false_positive),
        f64::from(true_positive) / f64::from(true_positive + false_negative),
        started.elapsed()
    );
    // The only misses allowed are the same-entity pairs that had one clue.
    let parked_same = slice
        .pairs
        .iter()
        .filter(|(family, pair)| in_held_out_split(*family) && family % 6 == 2 && pair.same)
        .filter(|(_, pair)| sample.iter().any(|graded| graded.left == pair.left))
        .count();
    assert_eq!(false_positive, 0);
    assert_eq!(false_negative as usize, parked_same);
    assert!(parked_same > 0, "the sample must include parked matches");
}

fn prove_places(root: &Path, properties: usize) {
    let started = Instant::now();
    let slice = places_slice(properties);
    let session = Session::new(
        root.join("places"),
        PLACE_RULES,
        &[("building", "BLD"), ("parcel", "PRC")],
    );
    run_stages(&session, &slice, "places");

    assert!(session.escrow("building", "pending.jsonl").len() >= slice.unsettled);
    assert_eq!(
        session.escrow("building", "cannot_link.jsonl").len(),
        slice.vetoed.len()
    );

    let of = |prefix: &str| -> Vec<&str> {
        slice
            .truth
            .iter()
            .filter(|(_, entity)| entity.ends_with("parcel") == (prefix == "parcel"))
            .map(|(spelling, _)| spelling.as_str())
            .collect()
    };
    let buildings = export_and_replay(&session, &slice, "building", &of("building"));
    let parcels = export_and_replay(&session, &slice, "parcel", &of("parcel"));

    let mut matched = 0;
    for (property, pair) in &slice.pairs {
        let (left, right) = (&buildings[&pair.left], &buildings[&pair.right]);
        let merged = left.is_some() && left == right;
        assert_eq!(merged, matches!(property % 4, 0 | 3), "property {property}");
        matched += usize::from(merged);
    }
    assert_eq!(
        parcels.values().filter(|id| id.is_some()).count(),
        parcels.len(),
        "every keyed parcel record resolves"
    );
    eprintln!(
        "[places] {} observations; {matched} of {properties} building pairs merged, \
         {} vetoed, {} parked; {:.1?}",
        slice.observations.len(),
        slice.vetoed.len(),
        slice.unsettled,
        started.elapsed()
    );
}

#[test]
fn four_workers_settle_names_without_loss_or_false_merge() {
    // 252 families give 101 held-out candidates.
    prove_names(TempDir::new().expect("tempdir").path(), 252);
}

#[test]
fn four_workers_settle_places_without_loss_or_false_merge() {
    prove_places(TempDir::new().expect("tempdir").path(), 40);
}

#[test]
#[ignore = "opt-in scale: about 3,700 name spellings and 500 properties"]
fn full_scale_swarm_proof() {
    let temp = TempDir::new().expect("tempdir");
    prove_names(temp.path(), 1_700);
    prove_places(temp.path(), 500);
}
