"""Arm D for Google Pryor OK: canon geo composition over a candidate universe of buildings.

Pipeline (same as outcomes/gdelt-cre-signals/canon site G): spec -> build_warehouse_rows.py -> canon geo
materialize-evidence -> compile-evidence -> solve. Rules were fixed before looking at the truth label:
  universe  Overture buildings >= 10,000 m2 within 1,300 m of the Foursquare "Google" POI (4581 Webb St)
  S_partition  (w5) nearer to the Google POI than to any named industrial Foursquare neighbor
  S_module     (w3) >= 3 footprints within +/-5% of each other, each >= 20,000 m2 (repeated hall module)
  S_crosslayer (w3) has a Microsoft GlobalML 2026-07 footprint within 30 m and within +/-25% area
The official permit point (ODEQ, 36.24250 -95.33020) is NOT used as evidence. It is used only to score afterward.
Query receipts (vid): Overture f8d00bb7..., Microsoft 5b665b04..., Foursquare 5ce01633... (see notes).
"""
import json
import subprocess
import sys
from math import asin, cos, radians, sin, sqrt
from pathlib import Path

HERE = Path(__file__).parent
BUILDER = "/Users/zac/Source/cmdrvl/outcomes/gdelt-cre-signals/canon/build_warehouse_rows.py"
PIN = (36.23959, -95.33284)          # Foursquare "Google", 4581 Webb St, Pryor (release 2026-08-11)
TRUTH = (36.24250, -95.33020)        # ODEQ permit memo facility point; scoring only

# Overture buildings >= 2,000 m2 within 1,500 m of PIN: (id, m2, lat, lon). Query vid f8d00bb78b088f2d...
OV = [
    ("154131f6-5edf-4f77-878f-f7b920f12870", 129308, 36.24499, -95.3248),
    ("215d1320-3552-4a36-9f93-64bacc0aae26", 60015, 36.23848, -95.32437),
    ("bd06b2c1-ffe1-4546-a087-82995dbc0dd6", 31093, 36.24144, -95.32299),
    ("3fe30483-2bb1-45bc-8737-985c7c402979", 30343, 36.23899, -95.32086),
    ("ee97c483-5b8d-4e97-b446-d4eeac619a08", 29321, 36.23734, -95.32219),
    ("3b5e6819-288d-44c1-85b3-b91841da4bb3", 13249, 36.24728, -95.32108),
    ("72ea9c27-e1c8-4096-8a95-080164733d83", 13073, 36.24519, -95.33194),
    ("db71b13c-6bde-4851-bb59-21b7548da559", 12961, 36.2473, -95.32396),
    ("56e84c8d-53c5-40e0-b785-8b111c29c5fb", 11642, 36.24117, -95.33005),
    ("dcc29a09-824b-47a1-9e95-ec6bc54df58d", 10929, 36.24016, -95.33626),
    ("37eade67-3e5f-429f-8359-21857a50e4e5", 10340, 36.24138, -95.32032),
]
# Microsoft GlobalML (release 2026-07-24) >= 2,000 m2 within 1,500 m of PIN: (m2, lat, lon). Query vid 5b665b04...
MS = [
    (128299, 36.24501, -95.32474), (60892, 36.23852, -95.32436), (35722, 36.23901, -95.32098),
    (32718, 36.23731, -95.32219), (32278, 36.24145, -95.32298), (13409, 36.24728, -95.32107),
    (13184, 36.24728, -95.32396), (11256, 36.24537, -95.33186), (11212, 36.24017, -95.33627),
    (10422, 36.24139, -95.32031), (8423, 36.24308, -95.33571), (6420, 36.24712, -95.33187),
    (6064, 36.24639, -95.3353), (5008, 36.24065, -95.3301), (4731, 36.24168, -95.33382),
    (3642, 36.24665, -95.31906), (2968, 36.24046, -95.33382), (2918, 36.23917, -95.33382),
    (2814, 36.24173, -95.33612), (2720, 36.24187, -95.33007),
]
# Other named industrial Foursquare places near the pin (factories, machine shop). Query vid 5ce01633...
INDUSTRIAL_NEIGHBORS = {
    "Red Devil (4175 Webb St)": (36.24529, -95.33182),
    "Harbison Walker International": (36.24593, -95.33524),
    "A.P. Green Industries": (36.24634, -95.33574),
    "Orchid Paper Products": (36.2488, -95.33007),
    "Praxair Welding Gas and Supply": (36.24677, -95.33313),
}


def hv(a, b):
    p1, p2 = radians(a[0]), radians(b[0])
    x = sin((p2 - p1) / 2) ** 2 + cos(p1) * cos(p2) * sin(radians(b[1] - a[1]) / 2) ** 2
    return 2 * 6371008.8 * asin(sqrt(x))


def run(cmd, out_path=None):
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"FAILED {' '.join(cmd)}\n{p.stderr[:2000]}")
    if out_path:
        Path(out_path).write_text(p.stdout)
    return p.stdout


def main():
    cands = []
    for oid, m2, lat, lon in OV:
        d_pin = hv(PIN, (lat, lon))
        if m2 >= 10000 and d_pin <= 1300:
            cands.append({"id": "overture:" + oid[:8], "oid": oid, "m2": m2, "lat": lat, "lon": lon, "m_pin": round(d_pin)})
    print("universe:", len(cands), "buildings")
    for c in cands:
        d_nb = min((hv((c["lat"], c["lon"]), p), n) for n, p in INDUSTRIAL_NEIGHBORS.items())
        c["m_nearest_industrial"], c["nearest_industrial"] = round(d_nb[0]), d_nb[1]
        ms_hit = [(abs(hv((c["lat"], c["lon"]), (ml, mo))), mm2) for mm2, ml, mo in MS
                  if hv((c["lat"], c["lon"]), (ml, mo)) <= 30 and abs(mm2 - c["m2"]) / c["m2"] <= 0.25]
        c["ms_match"] = bool(ms_hit)
        c["sqft"] = round(c["m2"] * 10.7639)
    part = [c["id"] for c in cands if c["m_pin"] < c["m_nearest_industrial"]]
    # repeated module: >= 3 footprints within +/-5% of the median of a cluster, each >= 20,000 m2
    big = sorted([c for c in cands if c["m2"] >= 20000], key=lambda c: c["m2"])
    module = []
    for c in big:
        grp = [x for x in big if abs(x["m2"] - c["m2"]) / c["m2"] <= 0.05]
        if len(grp) >= 3 and len(grp) > len(module):
            module = grp
    module_ids = [c["id"] for c in module]
    cross = [c["id"] for c in cands if c["ms_match"]]
    print("partition (nearer Google POI than any industrial neighbor):", len(part), "of", len(cands))
    print("module (>=3 within +/-5%, >=20k m2):", len(module_ids), [c["m2"] for c in module])
    print("cross-layer (Microsoft footprint within 30 m, +/-25% area):", len(cross), "of", len(cands))
    by_id = {c["id"]: c for c in cands}

    def src(sid, dataset, lineage, method, role, weight, members):
        return {"id": sid, "dataset": dataset, "release": "hot", "lineage": lineage, "method": method,
                "claim_role": role, "weight": weight, "reason": "membership method not calibrated",
                "observation": {"kind": "exact_sets", "level": "building", "sets": [members]},
                "records": [{"id": m, **{k: by_id[m][k] for k in ("oid", "m2", "sqft", "m_pin", "nearest_industrial", "m_nearest_industrial", "ms_match")}} for m in members]}

    spec = {
        "site": "d-google-pryor",
        "max_assignments": 1 << 16, "max_materialized_models": 1 << 14,
        "universe_policy": "Overture buildings >= 10,000 m2 within 1,300 m of the Foursquare Google POI (4581 Webb St)",
        "candidates": [{"id": c["id"], "record": {k: c[k] for k in ("oid", "m2", "sqft", "m_pin")}} for c in cands],
        "sources": [
            src("fsq-industrial-partition", "Foursquare OS Places 2026-08-11 (Google POI vs named industrial neighbors)",
                "EDGAR_DB.SOURCE.FOURSQUARE_OS_PLACES_HOT", "nearer_google_point_than_named_industrial_neighbor",
                "stable_identity_anchor", 5, part),
            src("hall-module-morphology", "Overture Maps buildings 2026-07", "EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT",
                "at_least_3_footprints_within_5pct_area_each_ge_20000m2", "attribute_observation", 3, module_ids),
            src("ms-crosslayer", "Microsoft GlobalML building footprints 2026-07-24",
                "EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT",
                "microsoft_footprint_within_30m_and_25pct_area", "attribute_observation", 3, cross),
        ],
    }
    (HERE / "pryor.spec.json").write_text(json.dumps(spec, indent=1))
    run(["uv", "run", "--quiet", "--with", "blake3", "python", BUILDER, str(HERE / "pryor.spec.json")],
        HERE / "pryor.warehouse_rows.json")
    run(["canon", "geo", "materialize-evidence", "--rows", str(HERE / "pryor.warehouse_rows.json")],
        HERE / "pryor.evidence_request.json")
    run(["canon", "geo", "compile-evidence", "--request", str(HERE / "pryor.evidence_request.json")],
        HERE / "pryor.evidence_compilation.json")
    run(["canon", "geo", "solve", "--request", str(HERE / "pryor.evidence_compilation.json")], HERE / "pryor.solve.json")
    solve = json.loads((HERE / "pryor.solve.json").read_text())
    print("\nSOLVE status:", solve["status"], "| summary:", {k: solve["summary"][k] for k in (
        "building_candidates", "structurally_feasible_assignments", "residual_model_count")})
    print("hard_forced:", solve["hard_forced"])
    print("soft_ranked models:", len(solve["soft_ranked"]))
    top = solve["soft_ranked"][:5]
    for i, m in enumerate(top, 1):
        print(f"  rank {i}:", json.dumps(m)[:260])
    # score against truth AFTER the solve (truth never entered the evidence)
    print("\nDistance of each candidate to the official permit point (m), membership in each source set:")
    for c in sorted(cands, key=lambda c: hv(TRUTH, (c["lat"], c["lon"]))):
        print(f"  {c['id']}  {c['sqft']:>9} sqft  truth_dist={hv(TRUTH, (c['lat'], c['lon'])):5.0f} m  pin_dist={c['m_pin']:>5}"
              f"  partition={'Y' if c['id'] in part else '-'} module={'Y' if c['id'] in module_ids else '-'}"
              f" ms={'Y' if c['id'] in cross else '-'}  nearest_ind={c['nearest_industrial']} ({c['m_nearest_industrial']} m)")


if __name__ == "__main__":
    main()
