#![forbid(unsafe_code)]

//! bd-j7u0: places are first-class in the shared session solver.
//!
//! Every flow runs the public binary through
//! `init -> ingest -> assert -> solve -> commit -> exact lookup` over an
//! invented multi-deal property fixture (1633 Broadway / Paramount Plaza). The
//! APN `1022-0043` is the identifier the bead names; nothing here is county
//! data. Negative tests prove a conflicting APN, disjoint parcels and a
//! cross-grain merge are contradicted or refused rather than merged.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  apn: {authoritative: true, single_valued: true}
  bin: {authoritative: true, single_valued: true}
  unit_id: {authoritative: true, single_valued: true}
  site_id: {authoritative: true, single_valued: true}
  address_id: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias, same_apn, same_parcel_id]
tier_b:
  eligible_evidence_types: [address_canonical_match, point_in_parcel, footprint_overlap, deed_chain_link, assessor_corroboration]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

const GRAINS: [(&str, &str); 5] = [
    ("address", "ADR"),
    ("building", "BLD"),
    ("parcel", "PRC"),
    ("site", "SIT"),
    ("unit", "UNT"),
];

const HASH: &str = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn p(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON stdout ({error}): stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run(args: &[&str], expected_exit: i32) -> std::process::Output {
    let output = canon().args(args).output().expect("canon runs");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "args={args:?} stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn observation(obs_id: &str, grain: &str, surface: &str, lineage: &str, keys: Value) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": grain,
        "surface": surface,
        "keys": keys,
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage}
    }))
    .expect("observation JSON")
}

fn support(kind: &str, left: &str, right: &str, basis: Value, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": basis,
        "source": {
            "channel": "fixture",
            "id": lineage,
            "lineage": lineage,
            "locator": format!("{lineage}#{left}:{right}"),
            "content_hash": HASH
        },
        "asserted_by": "agent:place-fixture"
    }))
    .expect("support JSON")
}

fn address_match(left: &str, right: &str, lineage: &str) -> String {
    support(
        "address_canonical_match",
        left,
        right,
        json!({"canonical_address": "1633 BROADWAY NEW YORK NY 10019", "normalizer": "fixture-usps-v1"}),
        lineage,
    )
}

fn footprint(left: &str, right: &str, lineage: &str) -> String {
    support(
        "footprint_overlap",
        left,
        right,
        json!({"method": "ST_INTERSECTS", "overlap_ratio": 0.93}),
        lineage,
    )
}

fn relation(kind: &str, left: &str, right: &str, subject: &str) -> String {
    serde_json::to_string(&json!({
        "type": "related",
        "left": left,
        "right": right,
        "basis": {"relation": kind, "subject": subject},
        "source": {"channel": "fixture", "id": "hierarchy", "lineage": "hierarchy"},
        "asserted_by": "agent:place-fixture"
    }))
    .expect("relation JSON")
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    session: PathBuf,
    rules: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path().to_path_buf();
        let mut map = String::from("version: canon.session.registries.v1\nkinds:\n");
        for (grain, prefix) in GRAINS {
            map.push_str(&format!(
                "  {grain}:\n    registry: {}\n    registry_id: {grain}s\n    prefix: {prefix}\n    zero_pad: 3\n    canonical_type: {grain}\n    alias_file: aliases.json\n",
                root.join(grain).display()
            ));
        }
        let map_path = root.join("registries.yaml");
        fs::write(&map_path, map).expect("registries map");
        let rules = root.join("rules.yaml");
        fs::write(&rules, RULES).expect("rules");
        let session = root.join("session");
        run(
            &[
                "entity",
                "session",
                "init",
                "--goal",
                "place identity",
                "--work-dir",
                p(&session),
                "--registries",
                p(&map_path),
            ],
            0,
        );
        Self {
            _temp: temp,
            root,
            session,
            rules,
        }
    }

    fn write(&self, name: &str, lines: &[String]) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture file");
        path
    }

    fn ingest(&self, lines: &[String]) {
        let input = self.write("observations.jsonl", lines);
        run(
            &["entity", "session", "ingest", p(&self.session), p(&input)],
            0,
        );
    }

    fn assert_evidence(&self, lines: &[String], expected_exit: i32) -> Value {
        let input = self.write("evidence.jsonl", lines);
        stdout_json(&run(
            &["entity", "session", "assert", p(&self.session), p(&input)],
            expected_exit,
        ))
    }

    fn solve(&self) -> Value {
        stdout_json(&run(
            &[
                "entity",
                "session",
                "solve",
                p(&self.session),
                "--rules",
                p(&self.rules),
            ],
            0,
        ))
    }

    fn commit(&self) -> Value {
        stdout_json(&run(
            &[
                "entity",
                "session",
                "commit",
                p(&self.session),
                "--bump",
                "patch",
            ],
            0,
        ))
    }

    fn registry(&self, grain: &str) -> PathBuf {
        self.root.join(grain)
    }

    /// Exact lookup of one value in one grain's registry.
    fn lookup(&self, grain: &str, value: &str) -> Option<String> {
        let input = self.root.join("lookup.csv");
        fs::write(&input, format!("name\n\"{value}\"\n")).expect("lookup input");
        let output = canon()
            .arg(&input)
            .arg("--registry")
            .arg(self.registry(grain))
            .arg("--column")
            .arg("name")
            .args(["--emit", "json", "--explicit", "--plain-json-values"])
            .output()
            .expect("lookup runs");
        let payload: Value = serde_json::from_slice(&output.stdout).expect("lookup JSON");
        payload["mappings"][0]["canonical_id"]
            .as_str()
            .map(str::to_string)
    }

    /// Every committed relation fact across the grain registries, with the
    /// grain whose registry holds it.
    fn relation_facts(&self) -> Vec<(String, Value)> {
        let mut facts = Vec::new();
        for (grain, _) in GRAINS {
            let path = self.registry(grain).join("relations.jsonl");
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                facts.push((
                    grain.to_string(),
                    serde_json::from_str(line).expect("relation fact"),
                ));
            }
        }
        facts
    }

    fn cannot_link(&self, grain: &str) -> Vec<Value> {
        fs::read_to_string(self.registry(grain).join("_escrow/cannot_link.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("cannot-link row"))
            .collect()
    }

    fn lint_all(&self) {
        for (grain, _) in GRAINS {
            run(
                &[
                    "registry",
                    "lint",
                    p(&self.registry(grain)),
                    "--emit",
                    "json",
                ],
                0,
            );
        }
    }
}

fn find_fact<'a>(
    facts: &'a [(String, Value)],
    subject: &str,
    relation: &str,
    object: &str,
) -> Option<&'a (String, Value)> {
    facts.iter().find(|(_, fact)| {
        fact["subject"]["identity_id"] == subject
            && fact["relation"]["value"] == relation
            && fact["object"]["identity_id"] == object
    })
}

fn contradictions_with<'a>(solve: &'a Value, reason: &str) -> Vec<&'a Value> {
    solve["contradictions"]
        .as_array()
        .expect("contradictions")
        .iter()
        .filter(|item| item["reason"] == reason)
        .collect()
}

fn cluster_with_surface<'a>(solve: &'a Value, surface: &str) -> Vec<&'a Value> {
    solve["clusters"]
        .as_array()
        .expect("clusters")
        .iter()
        .filter(|cluster| {
            cluster["surfaces"]
                .as_array()
                .expect("surfaces")
                .iter()
                .any(|candidate| candidate == surface)
        })
        .collect()
}

/// The multi-deal property fixture: three deal sources and three public
/// sources describe one tower (parcel, building, address, units, site) and one
/// distinct nearby building on its own parcel. "1633 Broadway" is both the
/// building's name and an address surface: same string, two grains.
fn property_observations() -> Vec<String> {
    vec![
        // Parcels: three sources share one APN; one carries none.
        observation(
            "p-county",
            "parcel",
            "Parcel 1022-0043",
            "county-parcels",
            json!({"apn": "1022-0043"}),
        ),
        observation(
            "p-alpha",
            "parcel",
            "APN 1022-0043",
            "deal-alpha-appraisal",
            json!({"apn": "1022-0043"}),
        ),
        observation(
            "p-beta",
            "parcel",
            "Block 1022 Lot 43",
            "deal-beta-tape",
            json!({"apn": "1022-0043"}),
        ),
        observation(
            "p-assessor",
            "parcel",
            "Tax Lot 43 Block 1022",
            "assessor-roll",
            json!({}),
        ),
        observation(
            "p-next",
            "parcel",
            "Parcel 1022-0044",
            "county-parcels",
            json!({"apn": "1022-0044"}),
        ),
        // Buildings: the tower under two names, and a distinct neighbour.
        observation(
            "b-alpha",
            "building",
            "1633 Broadway",
            "deal-alpha-tape",
            json!({}),
        ),
        observation(
            "b-beta",
            "building",
            "Paramount Plaza",
            "deal-beta-tape",
            json!({}),
        ),
        observation(
            "b-next",
            "building",
            "1675 Broadway",
            "deal-gamma-tape",
            json!({"bin": "BIN-1675"}),
        ),
        // Addresses: the same string as the building name, plus the full form.
        observation(
            "a-line",
            "address",
            "1633 Broadway",
            "deal-alpha-tape",
            json!({}),
        ),
        observation(
            "a-full",
            "address",
            "1633 Broadway, New York, NY 10019",
            "usps",
            json!({}),
        ),
        // Units and the site.
        observation(
            "u-3600",
            "unit",
            "1633 Broadway, Suite 3600",
            "deal-alpha-rentroll",
            json!({"unit_id": "1633B-3600"}),
        ),
        observation(
            "u-2500",
            "unit",
            "1633 Broadway, Suite 2500",
            "deal-alpha-rentroll",
            json!({"unit_id": "1633B-2500"}),
        ),
        observation(
            "s-pp",
            "site",
            "Paramount Plaza Site",
            "county-parcels",
            json!({"site_id": "SITE-PP"}),
        ),
    ]
}

fn property_evidence() -> Vec<String> {
    vec![
        // A parcel with no APN joins by a cited same_apn (tier A under the rules).
        support(
            "same_apn",
            "p-assessor",
            "p-county",
            json!({"apn": "1022-0043"}),
            "assessor-roll",
        ),
        // The tower under two names: two evidence types from two lineages (tier B).
        address_match("b-alpha", "b-beta", "usps-normalizer"),
        footprint("b-alpha", "b-beta", "overture-vs-county-footprints"),
        // The address surface in two forms.
        address_match("a-line", "a-full", "usps-normalizer"),
        support(
            "assessor_corroboration",
            "a-line",
            "a-full",
            json!({"field": "situs_address"}),
            "assessor-roll",
        ),
        // Grain-safe hierarchy: relations, never identity.
        relation("part_of", "u-3600", "b-alpha", "u-3600"),
        relation("contains_unit", "b-alpha", "u-2500", "b-alpha"),
        relation("located_on", "b-alpha", "p-county", "b-alpha"),
        relation("located_on", "a-line", "b-alpha", "a-line"),
        relation("located_on", "b-next", "p-next", "b-next"),
        relation("part_of", "b-alpha", "s-pp", "b-alpha"),
        relation("part_of", "p-county", "s-pp", "p-county"),
    ]
}

#[test]
fn place_session_resolves_grains_through_init_ingest_assert_solve_commit_and_exact_lookup() {
    let fixture = Fixture::new();
    fixture.ingest(&property_observations());
    let asserted = fixture.assert_evidence(&property_evidence(), 0);
    assert_eq!(asserted["rejected"], json!([]));
    assert_eq!(asserted["accepted"], 12);

    let solve = fixture.solve();
    assert_eq!(solve["summary"]["contradiction"], 0, "{solve}");
    assert_eq!(solve["summary"]["relations"], 7, "{solve}");
    // The tower is one building cluster (tier B), one address cluster (tier B),
    // and the three keyed parcel sources plus the assessor row are one parcel.
    let tower = cluster_with_surface(&solve, "Paramount Plaza");
    assert_eq!(tower.len(), 1);
    assert_eq!(tower[0]["outcome"], "accepted");
    assert_eq!(tower[0]["tier"], "B");
    let parcel = cluster_with_surface(&solve, "Tax Lot 43 Block 1022");
    assert_eq!(parcel.len(), 1);
    assert_eq!(parcel[0]["tier"], "A");
    assert_eq!(parcel[0]["surfaces"].as_array().expect("surfaces").len(), 4);
    // "1633 Broadway" is a cluster at two grains, never one.
    assert_eq!(cluster_with_surface(&solve, "1633 Broadway").len(), 2);

    let commit = fixture.commit();
    assert!(
        commit["registries"]
            .as_array()
            .is_some_and(|items| items.len() == 5),
        "{commit}"
    );

    // Exact lookup, per grain registry, identical to name lookup.
    let parcel_id = fixture
        .lookup("parcel", "Parcel 1022-0043")
        .expect("parcel resolves");
    for variant in [
        "APN 1022-0043",
        "Block 1022 Lot 43",
        "Tax Lot 43 Block 1022",
        "1022-0043",
    ] {
        assert_eq!(
            fixture.lookup("parcel", variant).as_deref(),
            Some(parcel_id.as_str()),
            "parcel variant {variant}"
        );
    }
    let next_parcel = fixture
        .lookup("parcel", "Parcel 1022-0044")
        .expect("neighbouring parcel resolves");
    assert_ne!(next_parcel, parcel_id, "the nearby parcel stays distinct");
    assert_eq!(
        fixture.lookup("parcel", "1022-0044").as_deref(),
        Some(next_parcel.as_str())
    );

    let building_id = fixture
        .lookup("building", "1633 Broadway")
        .expect("building resolves");
    assert_eq!(
        fixture.lookup("building", "Paramount Plaza").as_deref(),
        Some(building_id.as_str())
    );
    let next_building = fixture
        .lookup("building", "1675 Broadway")
        .expect("distinct nearby building resolves");
    assert_ne!(
        next_building, building_id,
        "the nearby building stays distinct"
    );
    assert_eq!(
        fixture.lookup("building", "BIN-1675").as_deref(),
        Some(next_building.as_str())
    );

    let address_id = fixture
        .lookup("address", "1633 Broadway")
        .expect("address resolves");
    assert_eq!(
        fixture
            .lookup("address", "1633 Broadway, New York, NY 10019")
            .as_deref(),
        Some(address_id.as_str())
    );
    let unit_id = fixture
        .lookup("unit", "1633 Broadway, Suite 3600")
        .expect("unit resolves");
    let other_unit = fixture
        .lookup("unit", "1633 Broadway, Suite 2500")
        .expect("second unit resolves");
    assert_ne!(unit_id, other_unit);
    let site_id = fixture
        .lookup("site", "Paramount Plaza Site")
        .expect("site resolves");

    // Each grain mints from its own id space; the same surface at two grains
    // is two ids, and one grain's registry does not know the other's spelling.
    assert!(address_id.starts_with("ADR-"), "{address_id}");
    assert!(building_id.starts_with("BLD-"), "{building_id}");
    assert!(parcel_id.starts_with("PRC-"), "{parcel_id}");
    assert!(unit_id.starts_with("UNT-"), "{unit_id}");
    assert!(site_id.starts_with("SIT-"), "{site_id}");
    assert_eq!(
        fixture.lookup("building", "1633 Broadway, New York, NY 10019"),
        None
    );
    assert_eq!(fixture.lookup("address", "Paramount Plaza"), None);
    assert_eq!(fixture.lookup("parcel", "1633 Broadway"), None);

    // The hierarchy is committed as typed relations between distinct ids.
    let facts = fixture.relation_facts();
    for (subject, name, object) in [
        (&unit_id, "part_of", &building_id),
        (&building_id, "contains_unit", &other_unit),
        (&building_id, "located_on", &parcel_id),
        (&address_id, "located_on", &building_id),
        (&next_building, "located_on", &next_parcel),
        (&building_id, "part_of", &site_id),
        (&parcel_id, "part_of", &site_id),
    ] {
        let found = find_fact(&facts, subject, name, object)
            .unwrap_or_else(|| panic!("relation {subject} {name} {object} missing: {facts:?}"));
        assert_eq!(
            facts.iter().filter(|(_, fact)| fact == &found.1).count(),
            1,
            "relation {subject} {name} {object} is written exactly once"
        );
    }
    assert_eq!(facts.len(), 7, "{facts:?}");
    // A relation between two grains is written once, by the kind that commits
    // later: parcel follows building, unit follows building.
    assert_eq!(
        find_fact(&facts, &building_id, "located_on", &parcel_id)
            .expect("located_on")
            .0,
        "parcel"
    );
    assert_eq!(
        find_fact(&facts, &unit_id, "part_of", &building_id)
            .expect("part_of")
            .0,
        "unit"
    );
    // Entity types on the facts name the grains, not an organisation.
    let located = &find_fact(&facts, &building_id, "located_on", &parcel_id)
        .expect("located_on")
        .1;
    assert_eq!(located["subject"]["entity_type"]["value"], "building");
    assert_eq!(located["object"]["entity_type"]["value"], "parcel");
    fixture.lint_all();

    // Committing the same solve again changes nothing: same ids, and no
    // relation is written a second time. (A fresh `solve` before the second
    // commit is a different case: each solve stamps its own hash into a
    // relation's provenance locator, so it appends a new fact per edge for
    // every relation kind, same-grain ones included. That is outside this
    // test's scope.)
    let registries_before = fixture.relation_facts();
    fixture.commit();
    assert_eq!(fixture.relation_facts(), registries_before);
    assert_eq!(
        fixture.lookup("building", "Paramount Plaza").as_deref(),
        Some(building_id.as_str())
    );
    assert_eq!(
        fixture.lookup("parcel", "1022-0043").as_deref(),
        Some(parcel_id.as_str())
    );
}

#[test]
fn conflicting_apns_are_contradicted_not_merged() {
    let fixture = Fixture::new();
    fixture.ingest(&[
        observation(
            "pa",
            "parcel",
            "Parcel 1022-0043",
            "county-parcels",
            json!({"apn": "1022-0043"}),
        ),
        observation(
            "pc",
            "parcel",
            "Parcel 1022-0099",
            "deal-gamma-tape",
            json!({"apn": "1022-0099"}),
        ),
        // The same support without any veto: the single-valued APN rule alone
        // must still refuse the merge.
        observation(
            "pe",
            "parcel",
            "Parcel 2000-0001",
            "county-parcels",
            json!({"apn": "2000-0001"}),
        ),
        observation(
            "pf",
            "parcel",
            "Parcel 2000-0002",
            "deal-gamma-tape",
            json!({"apn": "2000-0002"}),
        ),
    ]);
    fixture.assert_evidence(
        &[
            address_match("pa", "pc", "usps-normalizer"),
            footprint("pa", "pc", "overture-vs-county-footprints"),
            support(
                "conflicting_authoritative_apn",
                "pa",
                "pc",
                json!({"left_value": "1022-0043", "right_value": "1022-0099"}),
                "apn-comparison",
            ),
            address_match("pe", "pf", "usps-normalizer"),
            footprint("pe", "pf", "overture-vs-county-footprints"),
        ],
        0,
    );
    let solve = fixture.solve();

    let vetoed = cluster_with_surface(&solve, "Parcel 1022-0099");
    assert_eq!(vetoed.len(), 1, "{solve}");
    assert_eq!(vetoed[0]["outcome"], "contradiction");
    assert_eq!(vetoed[0]["reason"], "distinct");
    let reasons = vetoed[0]["conflict_reasons"].as_array().expect("reasons");
    assert!(
        reasons.contains(&json!("conflicting_authoritative_apn")),
        "{reasons:?}"
    );
    assert!(reasons.contains(&json!("key_conflict")), "{reasons:?}");
    assert_eq!(vetoed[0]["surfaces"].as_array().expect("surfaces").len(), 2);

    let unvetoed = cluster_with_surface(&solve, "Parcel 2000-0002");
    assert_eq!(unvetoed.len(), 1);
    assert_eq!(unvetoed[0]["outcome"], "contradiction");
    assert_eq!(unvetoed[0]["reason"], "key_conflict");
    assert_eq!(solve["summary"]["contradiction"], 2);

    fixture.commit();
    for surface in [
        "Parcel 1022-0043",
        "Parcel 1022-0099",
        "Parcel 2000-0001",
        "Parcel 2000-0002",
        "1022-0043",
        "1022-0099",
    ] {
        assert_eq!(
            fixture.lookup("parcel", surface),
            None,
            "{surface} must not be committed from a contradicted cluster"
        );
    }
    let rows = fixture.cannot_link("parcel");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["reason"], "conflicting_authoritative_apn");
    fixture.lint_all();
}

#[test]
fn disjoint_parcels_veto_what_tier_b_support_would_merge() {
    let parcels = [
        observation(
            "pg",
            "parcel",
            "Lot A (county)",
            "county-parcels",
            json!({}),
        ),
        observation(
            "ph",
            "parcel",
            "Lot A (assessor)",
            "assessor-roll",
            json!({}),
        ),
    ];
    let support_pair = [
        address_match("pg", "ph", "usps-normalizer"),
        support(
            "assessor_corroboration",
            "pg",
            "ph",
            json!({"field": "situs_address"}),
            "assessor-roll",
        ),
    ];

    // Control: the support alone merges the two parcels (tier B).
    let control = Fixture::new();
    control.ingest(&parcels);
    control.assert_evidence(&support_pair, 0);
    let merged = control.solve();
    assert_eq!(merged["summary"]["contradiction"], 0, "{merged}");
    let merged_cluster = cluster_with_surface(&merged, "Lot A (county)");
    assert_eq!(merged_cluster.len(), 1);
    assert_eq!(merged_cluster[0]["outcome"], "accepted");
    assert_eq!(merged_cluster[0]["tier"], "B");
    assert_eq!(
        merged_cluster[0]["surfaces"]
            .as_array()
            .expect("surfaces")
            .len(),
        2
    );

    // The same support plus a disjoint_parcels veto is a contradiction.
    let fixture = Fixture::new();
    fixture.ingest(&parcels);
    let mut with_veto = support_pair.to_vec();
    with_veto.push(support(
        "disjoint_parcels",
        "pg",
        "ph",
        json!({"distance_m": 450.0, "threshold_m": 25.0}),
        "spatial-join",
    ));
    fixture.assert_evidence(&with_veto, 0);
    let solve = fixture.solve();
    let contradicted = cluster_with_surface(&solve, "Lot A (county)");
    assert_eq!(contradicted.len(), 1, "{solve}");
    assert_eq!(contradicted[0]["outcome"], "contradiction");
    assert_eq!(contradicted[0]["reason"], "distinct");
    let reasons = contradicted[0]["conflict_reasons"]
        .as_array()
        .expect("reasons");
    assert!(reasons.contains(&json!("disjoint_parcels")), "{reasons:?}");
    assert!(!reasons.contains(&json!("key_conflict")), "{reasons:?}");
    assert_eq!(solve["summary"]["contradiction"], 1);

    fixture.commit();
    assert_eq!(fixture.lookup("parcel", "Lot A (county)"), None);
    assert_eq!(fixture.lookup("parcel", "Lot A (assessor)"), None);
    let rows = fixture.cannot_link("parcel");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["reason"], "disjoint_parcels");
    fixture.lint_all();
}

#[test]
fn cross_grain_merges_are_contradicted_and_only_hierarchy_crosses_grains() {
    let fixture = Fixture::new();
    fixture.ingest(&[
        observation(
            "b-1",
            "building",
            "1633 Broadway",
            "deal-alpha-tape",
            json!({"bin": "BIN-1633"}),
        ),
        observation(
            "a-1",
            "address",
            "1633 Broadway, New York, NY 10019",
            "usps",
            json!({"address_id": "ADDR-1633"}),
        ),
        observation(
            "r-1",
            "parcel",
            "Parcel 1022-0043",
            "county-parcels",
            json!({"apn": "1022-0043"}),
        ),
        observation(
            "u-1",
            "unit",
            "1633 Broadway, Suite 3600",
            "deal-alpha-rentroll",
            json!({"unit_id": "1633B-3600"}),
        ),
    ]);
    fixture.assert_evidence(
        &[
            // One identity claimed across two grains: contradicted.
            address_match("b-1", "a-1", "usps-normalizer"),
            support(
                "point_in_parcel",
                "a-1",
                "r-1",
                json!({"method": "ST_CONTAINS"}),
                "spatial-join",
            ),
            // A relation in the wrong direction, and one outside the
            // hierarchy vocabulary: not hierarchy either.
            relation("part_of", "b-1", "u-1", "b-1"),
            relation("parent_of", "b-1", "r-1", "b-1"),
            // A veto between different grains agrees with the grains.
            support(
                "grain_mismatch",
                "b-1",
                "a-1",
                json!({"left_grain": "building", "right_grain": "address"}),
                "grain-check",
            ),
            // The right way to relate an address to a building.
            relation("located_on", "a-1", "b-1", "a-1"),
        ],
        0,
    );
    let solve = fixture.solve();

    let mismatches = contradictions_with(&solve, "grain_mismatch");
    assert_eq!(mismatches.len(), 4, "{solve}");
    assert_eq!(solve["summary"]["contradiction"], 4);
    for mismatch in &mismatches {
        assert_eq!(mismatch["evidence_ids"].as_array().expect("ids").len(), 1);
    }
    // Only the hierarchy relation survives.
    assert_eq!(solve["summary"]["relations"], 1, "{solve}");
    assert_eq!(solve["relations"][0]["relation"], "located_on");
    assert_eq!(solve["relations"][0]["subject_entity_type"], "address");
    assert_eq!(solve["relations"][0]["object_entity_type"], "building");
    // Nothing merged across grains: four clusters, one per observation.
    assert_eq!(solve["summary"]["cluster_count"], 4);
    for cluster in solve["clusters"].as_array().expect("clusters") {
        assert_eq!(cluster["surfaces"].as_array().expect("surfaces").len(), 1);
    }

    fixture.commit();
    let building = fixture
        .lookup("building", "1633 Broadway")
        .expect("building");
    let address = fixture
        .lookup("address", "1633 Broadway, New York, NY 10019")
        .expect("address");
    assert_ne!(building, address);
    assert_eq!(
        fixture.lookup("building", "1633 Broadway, New York, NY 10019"),
        None
    );
    assert_eq!(fixture.lookup("address", "1633 Broadway"), None);
    let facts = fixture.relation_facts();
    assert_eq!(facts.len(), 1, "{facts:?}");
    assert!(find_fact(&facts, &address, "located_on", &building).is_some());
    // A grain mismatch is an error in the evidence, not a review question.
    for (grain, _) in GRAINS {
        assert!(
            !fixture
                .registry(grain)
                .join("_escrow/pending.jsonl")
                .is_file()
                || fs::read_to_string(fixture.registry(grain).join("_escrow/pending.jsonl"))
                    .unwrap_or_default()
                    .trim()
                    .is_empty(),
            "{grain} must hold no pending record for a grain mismatch"
        );
    }
    fixture.lint_all();
}

#[test]
fn place_assertions_without_an_inspectable_basis_are_rejected_at_assert() {
    let fixture = Fixture::new();
    fixture.ingest(&[
        observation("x", "parcel", "Parcel X", "county-parcels", json!({})),
        observation("y", "parcel", "Parcel Y", "deal-alpha-tape", json!({})),
    ]);
    let uncited_apn = serde_json::to_string(&json!({
        "type": "same_apn", "left": "x", "right": "y",
        "basis": {"apn": "1022-0043"},
        "source": {"channel": "fixture", "id": "no-citation", "lineage": "no-citation"},
        "asserted_by": "agent:place-fixture"
    }))
    .expect("json");
    let cases = [
        (
            support(
                "conflicting_authoritative_apn",
                "x",
                "y",
                json!({"left_value": "1022-0043", "right_value": "1022-0043"}),
                "apn-comparison",
            ),
            "invalid_basis",
        ),
        (
            support(
                "disjoint_parcels",
                "x",
                "y",
                json!({"distance_m": 10.0, "threshold_m": 25.0}),
                "spatial-join",
            ),
            "invalid_basis",
        ),
        (
            support(
                "grain_mismatch",
                "x",
                "y",
                json!({"left_grain": "parcel", "right_grain": "parcel"}),
                "grain-check",
            ),
            "invalid_basis",
        ),
        (
            support(
                "address_canonical_match",
                "x",
                "y",
                json!({}),
                "usps-normalizer",
            ),
            "invalid_basis",
        ),
        (uncited_apn, "place_identifier_citation"),
    ];
    for (line, code) in cases {
        let output = fixture.assert_evidence(std::slice::from_ref(&line), 1);
        assert_eq!(output["accepted"], 0, "{line}");
        assert_eq!(output["rejected"][0]["code"], code, "{line} -> {output}");
    }
}
