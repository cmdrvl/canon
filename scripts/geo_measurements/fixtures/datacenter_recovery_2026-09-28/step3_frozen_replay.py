"""Step 3 kernel replay on the FROZEN PRYOR snapshot (2026-09-29).

Rebuilds the historical arm-D rules from the frozen bytes only (no warehouse, no labels, no official point) and runs the pinned canon binary, so the
first-broken-link finding no longer depends on the hand-transcribed run-02 numbers. Rules are the ones fixed before the original run, unchanged:
  universe      Overture buildings with area >= MIN_M2 within 1,300 m of the Foursquare 'Google' pin (A1)
  partition w5  nearer the pin than to any named industrial Foursquare neighbor
  module    w3  >= 3 footprints within +/-5% of each other, each >= 20,000 m2
  crosslayer w3 Microsoft footprint within 30 m and +/-25% area
Variants: MIN_M2 = 10000 (historical replay), 1000, 0 (no size filter). Outputs go to <outdir>; only summaries are meant to be committed.

Usage: python3 step3_frozen_replay.py <snapshot dir PRYOR> <canon binary> <warehouse-rows builder> <outdir>
"""
import hashlib
import json
import subprocess
import sys
from itertools import combinations
from math import asin, cos, radians, sin, sqrt
from pathlib import Path

snap, canon, builder, out = Path(sys.argv[1]), sys.argv[2], sys.argv[3], Path(sys.argv[4])
out.mkdir(parents=True, exist_ok=True)
INDUSTRIAL = ["Red Devil", "Harbison Walker International", "A.P. Green Industries", "Orchid Paper Products", "Praxair Welding Gas and Supply"]
HIST = {"154131f6", "215d1320", "37eade67", "3fe30483", "56e84c8d", "72ea9c27", "bd06b2c1", "db71b13c", "dcc29a09", "ee97c483"}


def rows(name):
    return [json.loads(l) for l in (snap / f"{name}.jsonl").read_text().splitlines()]


def hv(a, b):
    p1, p2 = radians(a[0]), radians(b[0])
    x = sin((p2 - p1) / 2) ** 2 + cos(p1) * cos(p2) * sin(radians(b[1] - a[1]) / 2) ** 2
    return 2 * 6371008.8 * asin(sqrt(x))


def run(cmd, path=None):
    p = subprocess.run(cmd, capture_output=True, text=True)
    if path:
        Path(path).write_text(p.stdout)
    return p


ov, ms, fsq = rows("overture_buildings"), rows("microsoft_globalml"), rows("foursquare_places")
pin = next((r["LATITUDE"], r["LONGITUDE"]) for r in fsq if r["NAME"] == "Google" and (r["ADDRESS"] or "").startswith("4581"))
ind = {r["NAME"]: (r["LATITUDE"], r["LONGITUDE"]) for r in fsq if any((r["NAME"] or "").startswith(n) for n in INDUSTRIAL)}
result = {"snapshot_manifest_sha256": hashlib.sha256((snap / "manifest.json").read_bytes()).hexdigest(), "pin": pin,
          "industrial_neighbors_found": sorted(ind), "variants": []}

for min_m2 in (10000, 1000, 0):
    cands = []
    for r in ov:
        c = (r["CENTROID_LAT"], r["CENTROID_LON"])
        m2, d = r["AREA_M2_DERIVED_GEODESIC"], hv(pin, c)
        if m2 >= min_m2 and d <= 1300:
            cands.append({"id": "overture:" + r["PROVIDER_FEATURE_ID"][:8], "oid": r["PROVIDER_FEATURE_ID"], "m2": round(m2), "sqft": round(m2 * 10.7639),
                          "lat": c[0], "lon": c[1], "m_pin": round(d)})
    ids = [c["id"] for c in cands]
    for c in cands:
        near = min((hv((c["lat"], c["lon"]), p), n) for n, p in ind.items())
        c["m_nearest_industrial"], c["nearest_industrial"] = round(near[0]), near[1]
        c["ms_match"] = any(hv((c["lat"], c["lon"]), (m["CENTROID_LAT"], m["CENTROID_LON"])) <= 30
                            and abs(m["AREA_M2_DERIVED_GEODESIC"] - c["m2"]) / c["m2"] <= 0.25 for m in ms) if c["m2"] else False
    part = [c["id"] for c in cands if c["m_pin"] < c["m_nearest_industrial"]]
    big = sorted((c for c in cands if c["m2"] >= 20000), key=lambda c: c["m2"])
    module = []
    for c in big:
        grp = [x for x in big if abs(x["m2"] - c["m2"]) / c["m2"] <= 0.05]
        if len(grp) >= 3 and len(grp) > len(module):
            module = grp
    module_ids = [c["id"] for c in module]
    cross = [c["id"] for c in cands if c["ms_match"]]
    by_id = {c["id"]: c for c in cands}
    v = {"min_m2": min_m2, "universe": len(cands), "matches_historical_universe": (set(i.split(":")[1] for i in ids) == HIST) if min_m2 == 10000 else None,
         "sets": {"partition": len(part), "module": len(module_ids), "crosslayer": len(cross)}}
    # independent objective check (no canon): cost(S) = sum of weights of sources whose member set is not fully contained, per inclusion rewards
    weights = {"partition": (5, set(part)), "module": (3, set(module_ids)), "crosslayer": (3, set(cross))}
    forced = set().union(*[m for _, m in weights.values()])
    v["union_of_asserted_members"] = len(forced)
    v["full_universe_is_optimal_by_construction"] = True  # inclusion-only, w>=0: cost(universe) = 0 = minimum
    v["variant_dir"] = f"m2_{min_m2}"
    d = out / v["variant_dir"]
    d.mkdir(exist_ok=True)

    def src(sid, dataset, lineage, method, role, weight, members):
        return {"id": sid, "dataset": dataset, "release": "hot", "lineage": lineage, "method": method, "claim_role": role, "weight": weight,
                "reason": "membership method not calibrated", "observation": {"kind": "exact_sets", "level": "building", "sets": [members]},
                "records": [{"id": m, **{k: by_id[m][k] for k in ("oid", "m2", "sqft", "m_pin", "nearest_industrial", "m_nearest_industrial", "ms_match")}} for m in members]}
    spec = {"site": "frozen-pryor-" + str(min_m2), "max_assignments": 1 << 16, "max_materialized_models": 1 << 14,
            "universe_policy": f"Overture buildings >= {min_m2} m2 within 1,300 m of the Foursquare Google POI, from the frozen snapshot",
            "candidates": [{"id": c["id"], "record": {k: c[k] for k in ("oid", "m2", "sqft", "m_pin")}} for c in cands],
            "sources": [src("fsq-industrial-partition", "Foursquare OS Places 2026-08-11", "EDGAR_DB.SOURCE.FOURSQUARE_OS_PLACES_HOT",
                            "nearer_google_point_than_named_industrial_neighbor", "stable_identity_anchor", 5, part),
                        src("hall-module-morphology", "Overture Maps buildings 2026-07", "EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT",
                            "at_least_3_footprints_within_5pct_area_each_ge_20000m2", "attribute_observation", 3, module_ids),
                        src("ms-crosslayer", "Microsoft GlobalML building footprints 2026-07-24", "EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT",
                            "microsoft_footprint_within_30m_and_25pct_area", "attribute_observation", 3, cross)]}
    (d / "spec.json").write_text(json.dumps(spec, indent=1))
    steps = []
    for name, cmd, o in [("rows", ["uv", "run", "--quiet", "--with", "blake3", "python", builder, str(d / "spec.json")], d / "rows.json"),
                         ("materialize", [canon, "geo", "materialize-evidence", "--rows", str(d / "rows.json")], d / "request.json"),
                         ("compile", [canon, "geo", "compile-evidence", "--request", str(d / "request.json")], d / "compilation.json"),
                         ("solve", [canon, "geo", "solve", "--request", str(d / "compilation.json")], d / "solve.json")]:
        p = run(cmd, o)
        steps.append({"step": name, "exit": p.returncode, "stderr": p.stderr.strip()[:300]})
        if p.returncode:
            break
    v["steps"] = steps
    try:
        s = json.loads((d / "solve.json").read_text())
        v["solve"] = {"status": s.get("status"), "summary": s.get("summary"), "solve_sha256": hashlib.sha256((d / "solve.json").read_bytes()).hexdigest(),
                      "rank1_size": len(s["soft_ranked"][0]["model"]["buildings"]) if s.get("soft_ranked") else None,
                      "models_at_rank1_cost": sum(1 for m in s["soft_ranked"] if m["cost"] == s["soft_ranked"][0]["cost"]) if s.get("soft_ranked") else None}
    except (ValueError, KeyError, IndexError):
        v["solve"] = None
    result["variants"].append(v)
(out / "result.json").write_text(json.dumps(result, indent=2))
print(f"wrote {out / 'result.json'}")
