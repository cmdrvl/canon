"""Which of the 18 Franklin County parcels the Auditor classes 'data center' have an independent anchor?

Anchor = a Foursquare OS Places POI within 600 m whose NAME matches one FIXED brand list (decided before looking at
these parcels; applied identically to every parcel) or whose category says data center. The parcel's owner, class and
building area are NOT used to pick the search terms.
Read-only. Output: anchors.json
"""
import asyncio
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, "/Users/zac/.claude/skills/canon-geo-site-workup/scripts")
from site_worker import Mcp, q  # noqa: E402

HERE = Path(__file__).parent
# Fixed list, general hyperscaler / colo / neocloud brands. Same for every parcel.
BRANDS = ["amazon", "aws", "amazon web", "google", "meta ", "facebook", "microsoft", "azure", "oracle", "apple",
          "qts", "equinix", "digital realty", "cologix", "edgeconnex", "edged", "switch", "vantage", "stack infra",
          "cyrusone", "databank", "flexential", "coreweave", "iron mountain", "data center", "datacenter", "data centre"]
# (parcel id, lat, lon) from notes/truth_candidates_franklin_auditor.md; coordinates only, nothing else is used
PARCELS = [
    ("510-180711", 39.85252, -82.9968), ("050-011455", 40.04945, -83.1761), ("222-004984", 40.05831, -82.77106),
    ("050-011444", 40.05994, -83.1343), ("050-002806", 40.01894, -83.12438), ("610-207094", 40.11815, -82.99943),
    ("222-002127", 40.10607, -82.81577), ("610-210593", 40.11575, -83.00172), ("222-004355", 40.10614, -82.80121),
    ("222-002056", 40.1038, -82.81381), ("050-011895", 40.05211, -83.12718), ("222-004441", 40.10394, -82.8106),
    ("222-001940", 40.10424, -82.80514), ("222-004644", 40.10354, -82.80848), ("273-012619", 40.09216, -83.14368),
    ("222-004365", 40.10417, -82.80313), ("222-005361", 40.06964, -82.77164), ("050-011984", 40.0145, -83.12194),
]


async def main():
    out = []
    like = " OR ".join(f"NAME ILIKE '%{b}%'" for b in BRANDS)
    async with Mcp("cmdrvl-data") as data:
        for pid, lat, lon in PARCELS:
            pt = f"TO_GEOGRAPHY('POINT({lon} {lat})')"
            sql = f"""SELECT NAME, ADDRESS, ROUND(LATITUDE,5) LAT, ROUND(LONGITUDE,5) LON,
                ROUND(ST_DISTANCE(GEOM_GEOG,{pt})) M, FSQ_CATEGORY_LABELS_JSON CATS
                FROM SOURCE.FOURSQUARE_OS_PLACES_HOT
                WHERE REGION='OH' AND H3_R7 IN (SELECT H3_INT_TO_STRING(value) FROM TABLE(FLATTEN(
                    H3_GRID_DISK(H3_LATLNG_TO_CELL({lat}, {lon}, 7), 1))))
                AND DATE_CLOSED IS NULL AND ST_DWITHIN(GEOM_GEOG,{pt},600)
                AND (({like}) OR FSQ_CATEGORY_LABELS_JSON ILIKE '%data cent%')
                ORDER BY M LIMIT 8"""
            r, ms, vid = await data.call("edgar_data.run_query", q(sql))
            rows = (r or {}).get("rows", [])
            out.append({"parcel": pid, "lat": lat, "lon": lon, "vid": vid,
                        "pois": [{"name": x["NAME"], "address": x["ADDRESS"], "lat": x["LAT"], "lon": x["LON"],
                                  "m": x["M"], "cats": x["CATS"]} for x in rows]})
            best = rows[0] if rows else None
            print(f"{pid}  pois={len(rows)}  " + (f"nearest: {best['NAME']} ({best['ADDRESS']}) {best['M']} m" if best else "none"), flush=True)
    (HERE / "anchors.json").write_text(json.dumps(out, indent=1))
    n = sum(1 for o in out if o["pois"])
    print(f"\nparcels with >=1 brand/category POI within 600 m: {n} of {len(out)}")


asyncio.run(main())
