"""Step 3 helper for the data-center recovery protocol (docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md).

Works ONLY from the retained Pryor snapshot committed in
docs/geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d/ (pryor.spec.json and pryor.evidence_compilation.json).
No warehouse queries, no Canon binary, and no evaluation point are used. Standard library only.

It reports, for the current inclusion-only objective, what each retained source does to the ranking:
  cost(S) = sum of w_j over preferences whose member m_j is absent from S   (w_j >= 0)
by brute force over all 1,023 nonempty selections of the 10 candidates. It also lists which retained per-candidate
facts have no computational effect at all in the current representation.

Usage: python3 step3_snapshot_trace.py <repo-root> > step3_snapshot_trace.json
Meaning: objective reconstruction and evidence bookkeeping, not a site-accuracy measurement.
"""
import hashlib
import itertools
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
run = root / "docs/geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d"
spec_raw = (run / "pryor.spec.json").read_bytes()
comp_raw = (run / "pryor.evidence_compilation.json").read_bytes()
spec = json.loads(spec_raw)
comp = json.loads(comp_raw)
req = comp["composition_request"]
if req["universe"]["parcels"] or req.get("hard_constraints"):
    raise SystemExit("expected no parcels and no hard constraints")

ids = sorted(b["id"] for b in req["universe"]["buildings"])
cand = {c["id"]: c["record"] for c in spec["candidates"]}
sources = {s["id"]: s for s in spec["sources"]}

# preferences by source, read from the compiled request (what the solver actually sees)
prefs = req["soft_preferences"]
by_source = {sid: {} for sid in sources}
for p in prefs:
    sid = next(s for s in sources if f".{s}." in p["id"])
    by_source[sid][p["member"]["id"]] = p["cost_if_absent"]
assert sum(len(v) for v in by_source.values()) == len(prefs) == 17


def optimum(weights):
    """weights: {member: total weight}. Returns (min_cost, number of optimal selections, smallest optimal size)."""
    models = []
    for size in range(1, len(ids) + 1):
        for chosen in itertools.combinations(ids, size):
            s = set(chosen)
            models.append((sum(w for m, w in weights.items() if m not in s), size))
    best = min(c for c, _ in models)
    tied = [size for c, size in models if c == best]
    return best, len(tied), min(tied)


def merge(names):
    w = {}
    for n in names:
        for m, x in by_source[n].items():
            w[m] = w.get(m, 0) + x
    return w


ablation = []
names = list(sources)
for r in range(len(names) + 1):
    for combo in itertools.combinations(names, r):
        best, n_opt, smallest = optimum(merge(combo))
        ablation.append({"sources": list(combo), "min_cost": best, "optimal_selections": n_opt,
                         "smallest_optimal_selection_size": smallest})

support = merge(names)
table = []
for i in ids:
    rec = cand[i]
    recs = {sid: next((r for r in sources[sid]["records"] if r["id"] == i), None) for sid in sources}
    any_rec = next(r for r in recs.values() if r)
    table.append({
        "id": i, "sqft": rec["sqft"], "m2": rec["m2"], "distance_to_google_poi_m": rec["m_pin"],
        "nearest_named_industrial_neighbor": any_rec.get("nearest_industrial"),
        "distance_to_that_neighbor_m": any_rec.get("m_nearest_industrial"),
        "microsoft_footprint_match": any_rec.get("ms_match"),
        "asserted_by": [sid for sid in sources if i in by_source[sid]],
        "total_inclusion_reward": support.get(i, 0),
        "effect_of_being_closer_to_a_different_business_than_to_the_google_poi": 0,
    })

closer_to_other = [t["id"] for t in table
                   if t["distance_to_that_neighbor_m"] is not None and t["distance_to_that_neighbor_m"] < t["distance_to_google_poi_m"]]
out = {
    "purpose": "Step 3 evidence bookkeeping from the retained Pryor snapshot; not an accuracy measurement",
    "inputs": {"spec_sha256": hashlib.sha256(spec_raw).hexdigest(), "compilation_sha256": hashlib.sha256(comp_raw).hexdigest(),
               "universe": spec["universe_policy"], "candidates": len(ids), "soft_preferences": len(prefs)},
    "objective": "cost(S) = sum(w_j for absent m_j), w_j >= 0; hard constraints: none",
    "per_source_members": {sid: {"weight": sources[sid]["weight"], "asserted_members": len(m), "method": sources[sid]["method"],
                                 "claim_role": sources[sid]["claim_role"]} for sid, m in by_source.items()},
    "candidate_table": table,
    "candidates_closer_to_a_different_named_business_than_to_the_google_poi": closer_to_other,
    "ablation_by_source_subset": ablation,
}
print(json.dumps(out, indent=2))
