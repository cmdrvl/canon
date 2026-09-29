"""How much would a size band prune? (upper bound: assumes the stated acreage is exactly right)

For each of the 18 Franklin County parcels the Auditor classes 'data center' (truth labels), place an anchor 550 m
from the parcel centroid (550 m = median geocoder-vs-Foursquare anchor disagreement measured in run 01) at 8
compass bearings. Candidates = every Franklin parcel whose centroid is within 1,000 m of the anchor. Then apply an
acreage band around the TRUE acreage:
  wide   [0.5x, 2.6x]   (the band canon's D1 calibrated for building area; NYC, 13/16 truth in band)
  narrow [0.75x, 1.25x]
and count the candidates that survive. Nothing about class, owner or building area is used as a filter; ACRES only.
Read-only. Output: prune_results.json
"""
import asyncio
import json
import statistics as st
import sys
from math import cos, radians
from pathlib import Path

sys.path.insert(0, "/Users/zac/.claude/skills/canon-geo-site-workup/scripts")
from site_worker import Mcp, q  # noqa: E402

HERE = Path(__file__).parent
PARCELS = [  # id, lat, lon, acres (from notes/truth_candidates_franklin_auditor.md)
    ("510-180711", 39.85252, -82.9968, 480.4), ("050-011455", 40.04945, -83.1761, 95.0),
    ("222-004984", 40.05831, -82.77106, 221.9), ("050-011444", 40.05994, -83.1343, 55.2),
    ("050-002806", 40.01894, -83.12438, 143.3), ("610-207094", 40.11815, -82.99943, 7.3),
    ("222-002127", 40.10607, -82.81577, 14.8), ("610-210593", 40.11575, -83.00172, 7.6),
    ("222-004355", 40.10614, -82.80121, 28.5), ("222-002056", 40.1038, -82.81381, 19.0),
    ("050-011895", 40.05211, -83.12718, 15.0), ("222-004441", 40.10394, -82.8106, 12.0),
    ("222-001940", 40.10424, -82.80514, 7.8), ("222-004644", 40.10354, -82.80848, 9.2),
    ("273-012619", 40.09216, -83.14368, 4.4), ("222-004365", 40.10417, -82.80313, 7.1),
    ("222-005361", 40.06964, -82.77164, 78.2), ("050-011984", 40.0145, -83.12194, 9.1),
]
BEARINGS = [(0, 1), (45, 0.7071), (90, 1), (135, 0.7071), (180, 1), (225, 0.7071), (270, 1), (315, 0.7071)]
M_PER_DEG_LAT = 111320.0


def offset(lat, lon, bearing_deg, meters):
    from math import sin
    b = radians(bearing_deg)
    return lat + meters * cos(b) / M_PER_DEG_LAT, lon + meters * sin(b) / (M_PER_DEG_LAT * cos(radians(lat)))


async def main():
    out = []
    async with Mcp("cmdrvl-data") as data:
        for pid, lat, lon, acres in PARCELS:
            per = []
            for bearing, _ in BEARINGS:
                alat, alon = offset(lat, lon, bearing, 550)
                pt = f"TO_GEOGRAPHY('POINT({alon} {alat})')"
                # aggregate in SQL: a bare SELECT of rows is capped at 200 by the server and silently truncates
                r, ms, vid = await data.call("edgar_data.run_query", q(f"""
                    SELECT COUNT(ACRES) AS N,
                           COUNT_IF(ACRES BETWEEN {0.5 * acres} AND {2.6 * acres}) AS WIDE,
                           COUNT_IF(ACRES BETWEEN {0.75 * acres} AND {1.25 * acres}) AS NARROW,
                           COUNT_IF(PARCELID = '{pid}') AS TRUTH
                    FROM SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT
                    WHERE IS_CURRENT_RELEASE = TRUE AND PARCEL_KEY_STATUS = 'valid'
                    AND H3_R7 IN (SELECT H3_INT_TO_STRING(value) FROM TABLE(FLATTEN(
                        H3_GRID_DISK(H3_LATLNG_TO_CELL({alat}, {alon}, 7), 1))))
                    AND ST_DWITHIN(CENTROID_GEOG, {pt}, 1000)"""))
                x = ((r or {}).get("rows") or [{}])[0]
                per.append({"bearing": bearing, "vid": vid, "candidates": x.get("N"), "wide": x.get("WIDE"),
                            "narrow": x.get("NARROW"), "truth_in_candidates": (x.get("TRUTH") or 0) >= 1})
            out.append({"parcel": pid, "acres": acres, "runs": per})
            med = lambda k: st.median(p[k] for p in per)
            print(f"{pid} {acres:>6} ac  candidates(med)={med('candidates'):>5}  wide band={med('wide'):>5}  "
                  f"narrow band={med('narrow'):>4}  truth reached in {sum(p['truth_in_candidates'] for p in per)}/8 anchors",
                  flush=True)
    (HERE / "prune_results.json").write_text(json.dumps(out, indent=1))
    allruns = [p for o in out for p in o["runs"]]
    print("\nAcross all", len(allruns), "anchor placements (18 parcels x 8 bearings):")
    print("  truth parcel inside the 1 km candidate set:", sum(p["truth_in_candidates"] for p in allruns), "/", len(allruns))
    for k, label in (("candidates", "no size band"), ("wide", "wide band 0.5x-2.6x"), ("narrow", "narrow band 0.75x-1.25x")):
        v = [p[k] for p in allruns if p["truth_in_candidates"]]
        print(f"  {label:26s} median survivors {st.median(v):>5}  mean {st.mean(v):>7.1f}  "
              f"unique (==1): {sum(1 for x in v if x == 1)}/{len(v)}  <=3: {sum(1 for x in v if x <= 3)}/{len(v)}")


asyncio.run(main())
