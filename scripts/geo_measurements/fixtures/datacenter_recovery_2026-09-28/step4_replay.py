"""Step 4D replay: entity-level evidence with and without evidence-justified opposition (2026-09-29).

RULES ARE FIXED BEFORE ANY LABEL-BASED SCORING (see step4_preregistration.md). No rule or weight below may change after scoring.

Inputs: a frozen snapshot, its physical-entity file (step4_entities.py), the target-operator token, two canon binaries (baseline = installed 0.14.0,
patched = the prefer-absent branch build), an output dir.

Universe: every physical entity within 1,300 m of any anchor that has a coordinate. No size filter.
Support (unchanged in kind from the arm-D rules, recomputed on entities):
  partition   w5  nearer its nearest anchor than to the nearest Foursquare place more than 30 m from every anchor
  module      w3  >= 3 entities within +/-5% of each other's area, each >= 20,000 m2
  crosslayer  w3  the entity has >= 2 distinct declared upstreams (lineage-collapsed: Overture rows citing Microsoft do not count twice)
Opposition (new, w5, prefer_absent): a Foursquare place that is not closed, whose name does not contain the target token, has its point inside a member footprint of the
  entity (containment, not proximity). Missing support never becomes opposition.
Readout (cross-check corrected 2026-09-29 before any scoring to account for the kernel's non-empty-selection requirement): candidate counts are too large to materialize. Because preferences are per-member additive and there are no hard constraints, the soft optimum is separable per
entity: include iff cost_if_present < cost_if_absent, exclude iff greater, tie otherwise. `core` = strictly included, `upper` = core + ties. The separable readout is
cross-checked against the kernel's own rank-1 on small subsets (materializable) before it is reported.

Usage: uv run --with shapely --with blake3 python3 step4_replay.py <snapshot> <entities.json> <TARGET_TOKEN> <canon_baseline> <canon_patched> <outdir>
"""
import hashlib
import itertools
import json
import subprocess
import sys
from math import asin, cos, radians, sin, sqrt
from pathlib import Path

import blake3
from shapely import wkt
from shapely.geometry import Point

snap, ents_path, TOKEN, canon_base, canon_new, out = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3].lower(), sys.argv[4], sys.argv[5], Path(sys.argv[6])
out.mkdir(parents=True, exist_ok=True)
W_PART, W_MOD, W_CROSS, W_OPP = 5, 3, 3, 5
RADIUS = 1300


def hv(a, b):
    p1, p2 = radians(a[0]), radians(b[0])
    x = sin((p2 - p1) / 2) ** 2 + cos(p1) * cos(p2) * sin(radians(b[1] - a[1]) / 2) ** 2
    return 2 * 6371008.8 * asin(sqrt(x))


manifest = json.loads((snap / "manifest.json").read_text())
anchors = [(a["id"], a["lat"], a["lon"]) for a in manifest["anchors"] if a.get("lat") is not None]
near_anchor = lambda p: min(hv((a[1], a[2]), p) for a in anchors)
ents = json.loads(ents_path.read_text())["entities"]
fsq = [json.loads(l) for l in (snap / "foursquare_places.jsonl").read_text().splitlines()]
other_places = [(r["LATITUDE"], r["LONGITUDE"]) for r in fsq if near_anchor((r["LATITUDE"], r["LONGITUDE"])) > 30]

uni = [e for e in ents if near_anchor((e["lat"], e["lon"])) <= RADIUS]
by_id = {e["id"]: e for e in uni}
for e in uni:
    e["m_anchor"] = near_anchor((e["lat"], e["lon"]))
    e["m_nearest_other_place"] = min((hv((e["lat"], e["lon"]), p) for p in other_places), default=float("inf"))
part = {e["id"] for e in uni if e["m_anchor"] < e["m_nearest_other_place"]}
big = sorted((e for e in uni if e["area_m2"] >= 20000), key=lambda e: e["area_m2"])
module = set()
for e in big:
    grp = [x for x in big if abs(x["area_m2"] - e["area_m2"]) / e["area_m2"] <= 0.05]
    if len(grp) >= 3 and len(grp) > len(module):
        module = {x["id"] for x in grp}
cross = {e["id"] for e in uni if len(e["independent_upstreams"]) >= 2}

# opposition: containment of a competing named Foursquare place in a member footprint
geoms = {}
for f, s, key in [("overture_buildings", "overture", "PROVIDER_FEATURE_ID"), ("microsoft_globalml", "microsoft", "PROVIDER_FEATURE_ID"), ("fema_usa_structures", "fema", "BUILD_ID")]:
    for line in (snap / f"{f}.jsonl").read_text().splitlines():
        r = json.loads(line)
        geoms[(s, str(r[key]))] = r["GEOM_WKT"]
places = [(Point(r["LONGITUDE"], r["LATITUDE"]), r["NAME"], r["FSQ_PLACE_ID"]) for r in fsq
          if not r.get("DATE_CLOSED") and TOKEN not in (r["NAME"] or "").lower() and near_anchor((r["LATITUDE"], r["LONGITUDE"])) > 30]
oppose, oppose_why = set(), {}
for e in uni:
    for m in e["members"]:
        g = wkt.loads(geoms[(m["source"], m["key"])])
        for pt, name, pid in places:
            if g.contains(pt):
                oppose.add(e["id"])
                oppose_why.setdefault(e["id"], []).append({"place": name, "fsq_place_id": pid, "footprint": f"{m['source']}:{m['key']}"})

contracts_src = [("partition", part, W_PART, "stable_identity_anchor"), ("module", module, W_MOD, "attribute_observation"), ("crosslayer", cross, W_CROSS, "attribute_observation")]


def build_request(candidates, with_opposition):
    cids = {e["id"] for e in candidates}
    contracts, observations = [], []
    for name, members, w, role in contracts_src:
        cid = f"rho.replay.{name}"
        contracts.append({"id": cid, "version": "1.0.0", "source_dataset": name, "source_release": "hot", "source_lineage_ids": [f"replay.{name}"], "method_id": name,
                          "method_version": "1.0.0", "claim_role": role,
                          "basis": {"kind": "uncalibrated", "reason": "membership method not calibrated", "admission_policy": {"kind": "soft_with_weight", "cost_if_absent": w}}})
        for m in sorted(members & cids):
            rid = f"replay.{name}.{m}"
            observations.append({"id": f"obs.{rid}", "contract_id": cid, "source_records": [{"source_record_id": rid, "source_vintage": "hot", "record_blake3": blake3.blake3(rid.encode()).hexdigest()}],
                                 "observation": {"kind": "prefer_member", "member": {"level": "building", "id": m}, "cost_if_absent": w}})
    if with_opposition:
        cid = "rho.replay.competing_occupant"
        contracts.append({"id": cid, "version": "1.0.0", "source_dataset": "competing_occupant", "source_release": "hot", "source_lineage_ids": ["replay.competing_occupant"],
                          "method_id": "named_other_place_inside_footprint", "method_version": "1.0.0", "claim_role": "attribute_observation",
                          "basis": {"kind": "uncalibrated", "reason": "membership method not calibrated", "admission_policy": {"kind": "soft_with_weight", "cost_if_absent": W_OPP}}})
        for m in sorted(oppose & cids):
            rid = f"replay.competing_occupant.{m}"
            observations.append({"id": f"obs.{rid}", "contract_id": cid, "source_records": [{"source_record_id": rid, "source_vintage": "hot", "record_blake3": blake3.blake3(rid.encode()).hexdigest()}],
                                 "observation": {"kind": "prefer_absent_member", "member": {"level": "building", "id": m}, "cost_if_present": W_OPP}})
    return {"version": "canon_geo_evidence_request.v0", "profile": {"version": "canon_geo_composition_profile.v0", "selection_level": "building"},
            "universe": {"parcels": [], "buildings": [{"id": e["id"], "parcel_ids": []} for e in sorted(candidates, key=lambda e: e["id"])]},
            "contracts": contracts, "observations": observations, "max_assignments": 1 << 16, "max_materialized_models": 1 << 14}


def run_canon(binary, req, tag):
    d = out / tag
    d.mkdir(exist_ok=True)
    (d / "request.json").write_text(json.dumps(req))
    res = {}
    for name, args, dest in [("compile", ["geo", "compile-evidence", "--request", str(d / "request.json")], d / "compilation.json"),
                              ("solve", ["geo", "solve", "--request", str(d / "compilation.json")], d / "solve.json")]:
        p = subprocess.run([binary, *args], capture_output=True, text=True)
        dest.write_text(p.stdout)
        res[name] = p.returncode
        if p.returncode:
            res[name + "_stderr"] = p.stderr[:300]
            return res, None
    return res, json.loads((d / "solve.json").read_text())


def separable(candidates, with_opposition):
    cost_in = {e["id"]: 0 for e in candidates}
    cost_out = {e["id"]: 0 for e in candidates}
    for name, members, w, _ in contracts_src:
        for m in members & cost_out.keys():
            cost_out[m] += w
    if with_opposition:
        for m in oppose & cost_in.keys():
            cost_in[m] += W_OPP
    core = {m for m in cost_in if cost_in[m] < cost_out[m]}
    tie = {m for m in cost_in if cost_in[m] == cost_out[m]}
    return core, tie, cost_in, cost_out


def kernel_rank1(solve):
    r = solve["soft_ranked"]
    if not r:
        return None
    best = r[0]["cost"]
    return best, sorted({tuple(m["model"]["buildings"]) for m in r if m["cost"] == best})


result = {"target_token": TOKEN, "anchors": len(anchors), "universe": len(uni), "support": {"partition": len(part), "module": len(module), "crosslayer": len(cross)},
          "opposition": len(oppose), "opposition_examples": {k: v for k, v in list(oppose_why.items())[:5]}, "weights": {"partition": W_PART, "module": W_MOD, "crosslayer": W_CROSS, "opposition": W_OPP}}
readouts = {}
for tag, with_opp, binary in [("baseline", False, canon_base), ("patched", True, canon_new)]:
    core, tie, cin, cout = separable(uni, with_opp)
    readouts[tag] = {"core": sorted(core), "tie": sorted(tie)}
    result[tag] = {"core": len(core), "tie_neutral_or_balanced": len(tie), "excluded": len(uni) - len(core) - len(tie), "upper": len(core) + len(tie)}
    full_res, full_solve = run_canon(binary, build_request(uni, with_opp), tag + "_full")
    result[tag]["kernel_full"] = {**full_res, "status": full_solve and full_solve["status"], "residual_model_count": full_solve and full_solve["summary"]["residual_model_count"],
                                  "ranked_models": full_solve and len(full_solve["soft_ranked"])}
    # kernel cross-check on subsets: up to 12 candidates, opposed and supported first, deterministic
    prio = sorted(uni, key=lambda e: (e["id"] not in oppose, e["id"] not in (part | module | cross), e["id"]))
    checks = []
    for size, offset in [(8, 0), (10, 0), (12, 0), (12, 12)]:
        sub = prio[offset:offset + size]
        if len(sub) < 3:
            continue
        r, solve = run_canon(binary, build_request(sub, with_opp), f"{tag}_check_{size}_{offset}")
        k = kernel_rank1(solve) if solve else None
        c2, t2, _, _ = separable(sub, with_opp)
        _, _, cin2, cout2 = separable(sub, with_opp)
        py_cost = sum(min(cin2[m], cout2[m]) for m in cin2)
        if not (c2 | t2):  # the kernel requires a non-empty selection: add the cheapest forced inclusion
            py_cost += min(cin2[m] - cout2[m] for m in cin2)
        ok = bool(k) and k[0] == py_cost and (all(set(model) >= c2 and set(model) <= (c2 | t2) for model in k[1]) if (c2 | t2) else True)
        checks.append({"size": len(sub), "kernel_rank1_cost": k and k[0], "separable_cost": py_cost, "kernel_rank1_models": k and len(k[1]), "agrees": ok})
    result[tag]["kernel_crosscheck"] = checks
(out / "readout_entities.json").write_text(json.dumps(readouts))
result["readout_sha256"] = hashlib.sha256((out / "readout_entities.json").read_bytes()).hexdigest()
(out / "result.json").write_text(json.dumps(result, indent=2))
print(json.dumps({k: v for k, v in result.items() if k != "opposition_examples"}, indent=1))
