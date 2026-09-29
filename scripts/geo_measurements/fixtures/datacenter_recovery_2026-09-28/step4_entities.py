"""Step 4A: physical-entity candidates from a frozen snapshot, with lineage kept (2026-09-29).

Rule fixed before any label-based scoring: two footprints (Overture / Microsoft GlobalML / FEMA) are the same physical entity when they belong to the same
connected component of the relation  intersection_area / min(area_a, area_b) >= 0.5  (planar, EPSG:4326 degrees squared; a ratio, so units cancel).
An entity keeps every member record's source, source key, area and raw source citations (Overture SOURCES_JSON), so source count and lineage stay separate:
`independent_upstreams` counts distinct declared upstream datasets, not rows. No size filter is applied. Retrieval bound is the snapshot's own neighborhood.

Usage: uv run --with shapely python3 step4_entities.py <snapshot dir> <out.json>
"""
import hashlib
import json
import sys
from pathlib import Path

from shapely import wkt
from shapely.strtree import STRtree

snap, out = Path(sys.argv[1]), Path(sys.argv[2])
UP = {"Microsoft ML Buildings": "microsoft", "OpenStreetMap": "osm", "USGS Lidar": "usgs_lidar"}  # Overture-declared upstreams, normalized so a Microsoft-derived Overture row and the Microsoft row share one name
SRC = {"overture_buildings": "overture", "microsoft_globalml": "microsoft", "fema_usa_structures": "fema"}
KEY = {"overture": "PROVIDER_FEATURE_ID", "microsoft": "PROVIDER_FEATURE_ID", "fema": "BUILD_ID"}
recs = []
for f, s in SRC.items():
    for line in (snap / f"{f}.jsonl").read_text().splitlines():
        r = json.loads(line)
        geom = wkt.loads(r["GEOM_WKT"])
        if not geom.is_valid:
            geom = geom.buffer(0)
        upstream = [s]
        if s == "overture":
            try:
                upstream = sorted({UP.get(x["dataset"], x["dataset"].lower().replace(" ", "_")) for x in json.loads(r["SOURCES_JSON"] or "[]")}) or ["overture"]
            except ValueError:
                upstream = ["overture"]
        recs.append({"source": s, "key": str(r[KEY[s]]), "area_m2": r["AREA_M2_DERIVED_GEODESIC"], "lat": r["CENTROID_LAT"], "lon": r["CENTROID_LON"],
                     "upstream": upstream, "names": r.get("NAMES_JSON"), "geom": geom})

tree = STRtree([r["geom"] for r in recs])
parent = list(range(len(recs)))


def find(i):
    while parent[i] != i:
        parent[i] = parent[parent[i]]
        i = parent[i]
    return i


for i, r in enumerate(recs):
    for j in tree.query(r["geom"]):
        if j <= i:
            continue
        inter = r["geom"].intersection(recs[j]["geom"]).area
        small = min(r["geom"].area, recs[j]["geom"].area)
        if small > 0 and inter / small >= 0.5:
            parent[find(i)] = find(j)

groups = {}
for i in range(len(recs)):
    groups.setdefault(find(i), []).append(i)
entities = []
for members in groups.values():
    ms = sorted((recs[i] for i in members), key=lambda r: (r["source"], r["key"]))
    eid = "ent:" + hashlib.sha256("|".join(f"{m['source']}:{m['key']}" for m in ms).encode()).hexdigest()[:10]
    big = max(ms, key=lambda m: m["area_m2"])
    upstreams = sorted({u for m in ms for u in m["upstream"]})
    entities.append({"id": eid, "area_m2": round(big["area_m2"], 1), "lat": big["lat"], "lon": big["lon"],
                     "sources": sorted({m["source"] for m in ms}), "record_count": len(ms),
                     "independent_upstreams": upstreams,
                     "names": [m["names"] for m in ms if m["names"]],
                     "members": [{"source": m["source"], "key": m["key"], "area_m2": round(m["area_m2"], 1), "upstream": m["upstream"]} for m in ms]})
entities.sort(key=lambda e: e["id"])
summary = {"records": len(recs), "entities": len(entities), "by_source_count": {},
           "single_upstream_entities_with_multiple_records": sum(1 for e in entities if e["record_count"] > 1 and len(e["independent_upstreams"]) == 1)}
for e in entities:
    k = "+".join(e["sources"])
    summary["by_source_count"][k] = summary["by_source_count"].get(k, 0) + 1
out.write_text(json.dumps({"snapshot": str(snap.name), "rule": "intersection/min-area >= 0.5, connected components", "summary": summary, "entities": entities}, indent=1))
print(json.dumps(summary))
