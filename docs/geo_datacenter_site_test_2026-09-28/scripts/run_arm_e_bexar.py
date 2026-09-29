"""Arm E on Bexar County TX: canon composition over LANDED PARCELS with a set-sum size band.

News (KSAT 2026-09-22, read via a fetch summarizer, unverified quote): "the 158-acre Microsoft Texas Research Park" in
Bexar County. Our news row kept the size (158-acre) but dropped the operator and park name.
Parcels: TxGIO 2025 Bexar county file (spot scrape 20260929T000034Z). Acres are derived from the polygon (EPSG:5070),
because Bexar's LEGAL_AREA is only 13% filled. There is NO independent truth label for the 158 acres, so this run
reports what canon enumerates and refuses; it is not an accuracy measurement.

Canon requests are authored directly (composition_request.v0, selection_level parcel) and solved with `canon geo solve`.
The band is a hypothesis about the article's size, NOT calibrated evidence: canon's doctrine reserves hard bands for
calibrated sources, so results are workbench output.
Rules fixed before running: bands +/-5%, +/-10%, +/-25% and canon's D1 NYC band 0.5x-2.6x; universes A, B, C below.
"""
import glob
import json
import subprocess
import sys
from itertools import combinations  # noqa: F401 - kept for readers reproducing counts by hand
from pathlib import Path

import pyogrio

HERE = Path(__file__).parent
CURVES = Path("/Users/zac/Source/cmdrvl/cmdrvl-curves")
SHP = glob.glob(str(CURVES / "local_data/spot_scrapers/txgio_land_parcels/20260929T000034Z/extracted/*/*.shp"))[0]
NEWS_ACRES = 158.0
BANDS = {"+/-5%": (0.95, 1.05), "+/-10%": (0.90, 1.10), "+/-25%": (0.75, 1.25), "D1 0.5x-2.6x": (0.5, 2.6)}
MAX_UNIVERSE = 16  # 2^16 assignments fits max_assignments below


def load():
    g = pyogrio.read_dataframe(SHP)
    g = g[g.geometry.notna() & ~g.geometry.is_empty].copy()
    p = g.to_crs(5070)
    g["acres"] = p.area / 4046.8564224
    c = p.geometry.centroid
    g["x"], g["y"] = c.x, c.y  # metres, EPSG:5070
    g = g.reset_index(drop=True)
    g["pid"] = [f"{gid}#{i}" for i, gid in enumerate(g.GEO_ID.fillna("NA"))]  # GEO_ID repeats in Bexar; index makes ids unique
    g["is_ms"] = g.OWNER_NAME.fillna("").str.contains("MICROSOFT", case=False)
    return g


def request(parcels, lo, hi, prefer=None, cost=3):
    ids = [r.pid for r in parcels.itertuples()]
    req = {
        "version": "canon_geo_composition_request.v0",
        "profile": {"version": "canon_geo_composition_profile.v0", "selection_level": "parcel"},
        "universe": {"parcels": ids, "buildings": []},
        "hard_constraints": [{
            "id": "hyp.news_size_band.ksat_158_acre",  # a HYPOTHESIS about the article's size, not calibrated evidence
            "constraint": {
                "kind": "integer_sum_band", "level": "parcel",
                "measure": {"semantic_id": "parcel_area_acres_x100", "unit": "acre/100", "value_origin": "exact_derived"},
                "values": [{"id": r.pid, "value": int(round(r.acres * 100))} for r in parcels.itertuples()],
                "min": int(round(lo * NEWS_ACRES * 100)), "max": int(round(hi * NEWS_ACRES * 100)),
            },
        }],
        "soft_preferences": [
            {"id": f"pref.owner.{r.pid}", "member": {"level": "parcel", "id": r.pid}, "cost_if_absent": cost}
            for r in parcels.itertuples() if prefer is not None and prefer(r)
        ],
        "max_assignments": 1 << 16, "max_materialized_models": 1 << 16,
    }
    return req


def solve(req, name):
    path = HERE / f"{name}.request.json"
    path.write_text(json.dumps(req))
    p = subprocess.run(["canon", "geo", "solve", "--request", str(path)], capture_output=True, text=True)
    if p.returncode != 0:
        return {"status": "canon_error", "stderr": p.stderr[:600]}
    out = json.loads(p.stdout)
    (HERE / f"{name}.solve.json").write_text(p.stdout)
    return out


def describe(res, by_id):
    st = res.get("status")
    summ = res.get("summary", {})
    models = [set(m["model"]["parcels"]) for m in res.get("soft_ranked", [])] or [set(m["parcels"]) for m in res.get("residual_models", [])]
    info = {"status": st, "feasible_models": summ.get("residual_model_count"), "candidates": summ.get("parcel_candidates")}
    if models:
        allsets = [set(m) for m in models]
        info["in_every_model"] = sorted(set.intersection(*allsets)) if allsets else []
        info["in_no_model"] = sorted(set(by_id) - set.union(*allsets)) if allsets else sorted(by_id)
        best = res["soft_ranked"][0] if res.get("soft_ranked") else None
        if best:
            info["rank1_cost"] = best["cost"]
            info["rank1_parcels"] = sorted(best["model"]["parcels"])
    if res.get("conflict_constraint_ids"):
        info["conflict_constraint_ids"] = res["conflict_constraint_ids"]
    return info


def label(pid, by_id):
    r = by_id[pid]
    return f"{r.acres:6.1f} ac {str(r.OWNER_NAME)[:24]:24s} {str(r.SITUS_ADDR)[:26]}"


def main():
    g = load()
    anchor = g[g.SITUS_ADDR.fillna("").str.contains("15434") & g.is_ms].iloc[0]  # Microsoft's Lambda Dr parcel (Epoch SAT40 street)
    dist = ((g.x - anchor.x) ** 2 + (g.y - anchor.y) ** 2) ** 0.5
    u_a = g[g.is_ms & (dist <= 2500) & (g.acres >= 1)]
    u_b = g[g.is_ms & (g.acres >= 4)]
    thr = 5
    while True:
        u_c = g[(dist <= 2000) & (g.acres >= thr)]
        if len(u_c) <= MAX_UNIVERSE:
            break
        thr += 1
    cases = {"A_microsoft_within_2.5km_of_Lambda": (u_a, None), "B_all_microsoft_bexar_ge4ac": (u_b, None),
             f"C_any_owner_within_2km_ge{thr}ac_prefer_microsoft": (u_c, lambda r: r.is_ms)}
    by_id = {r.pid: r for r in g.itertuples()}
    print(f"news size {NEWS_ACRES} acres | anchor parcel: {label(anchor.pid, by_id)}")
    report = {}
    for cname, (u, prefer) in cases.items():
        print(f"\n=== {cname}: {len(u)} parcels, {u.acres.sum():.1f} acres total")
        for r in u.sort_values("acres", ascending=False).itertuples():
            print("    ", label(r.pid, by_id))
        report[cname] = {}
        for bname, (lo, hi) in BANDS.items():
            res = solve(request(u, lo, hi, prefer), f"{cname}__{bname.replace('/', '').replace('%', 'pct').replace(' ', '')}")
            info = describe(res, {r.pid: r for r in u.itertuples()})
            report[cname][bname] = info
            line = f"  band {bname:13s} [{lo*NEWS_ACRES:6.1f},{hi*NEWS_ACRES:6.1f}] ac -> status {info['status']:10s} feasible sets {info.get('feasible_models')}"
            if info.get("rank1_parcels") is not None:
                line += f" | rank-1 cost {info['rank1_cost']}, {len(info['rank1_parcels'])} parcels, {sum(by_id[p].acres for p in info['rank1_parcels']):.1f} ac"
            print(line)
            if bname == "+/-10%":
                if info.get("in_every_model"):
                    print("     in EVERY feasible set:", [label(p, by_id) for p in info["in_every_model"]])
                if info.get("rank1_parcels") and prefer is not None:
                    print("     rank-1 set:", [label(p, by_id) for p in info["rank1_parcels"]])
    (HERE / "arm_e_report.json").write_text(json.dumps(report, indent=1, default=str))


if __name__ == "__main__":
    sys.exit(main())
