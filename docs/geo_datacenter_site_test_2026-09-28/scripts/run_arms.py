"""Arms A-C for the data center canon geo test (read-only).

A  plain geocoder on the address as given, plus a false-exact guard (matched street must equal input street)
B  A, else Foursquare address/name lookup for a point
C  around the anchor: large footprints (Microsoft 2026-07, Overture, FEMA 2023) within 1,500 m -> ranked candidates
Reuses the canon-geo-site-workup worker's MCP client. Never places a site from memory: every anchor cites a tool vid.
Usage: run_arms.py sites.json results.jsonl
"""
import asyncio
import json
import re
import sys
import time

sys.path.insert(0, "/Users/zac/.claude/skills/canon-geo-site-workup/scripts")
from site_worker import Mcp, q  # noqa: E402

ABBR = {"rd": "road", "st": "street", "ave": "avenue", "blvd": "boulevard", "dr": "drive", "ln": "lane", "ct": "court",
        "hwy": "highway", "pkwy": "parkway", "cir": "circle", "co": "county", "n": "north", "s": "south", "e": "east",
        "w": "west", "ne": "northeast", "nw": "northwest", "se": "southeast", "sw": "southwest", "pl": "place",
        "trl": "trail", "fm": "farm", "sh": "state"}
DIRS = {"north", "south", "east", "west", "northeast", "northwest", "southeast", "southwest"}


def toks(street):
    s = (street or "").lower()
    # state routes and highways are the same road under two names
    s = re.sub(r"\bstate (?:rte|route|rt|hwy|highway)\b", "highway", s)
    s = re.sub(r"\b(?:rte|route|rt)\b", "highway", s)
    t = re.sub(r"[^a-z0-9 ]", " ", s).split()
    return [ABBR.get(x, x) for x in t]


def guard(input_street, matched_address):
    """Accept a resolver 'exact' only if house number and street name agree. Returns (ok, reason).
    A house-number range in the input ('14436-14998 Fairview Rd') accepts any matched number inside it."""
    a = toks(input_street)
    m = toks((matched_address or "").split(",")[0])
    if not a or not m:
        return False, "empty"
    a_num = a[0] if a[0].isdigit() else None
    m_num = m[0] if m[0].isdigit() else None
    rng = re.match(r"^\s*(\d+)\s*[-–]\s*(\d+)\b", input_street or "")
    if rng and m_num:
        lo, hi = sorted((int(rng.group(1)), int(rng.group(2))))
        if lo <= int(m_num) <= hi:
            a = [m_num] + a[2:]  # range consumed two numeric tokens; treat the matched number as the input's
            a_num = m_num
    if a_num != m_num:
        return False, f"house number differs (input {a_num!r} vs matched {m_num!r})"
    a_name = [x for x in (a[1:] if a_num else a) if x not in DIRS]
    m_name = [x for x in (m[1:] if m_num else m) if x not in DIRS]
    if a_name[:1] != m_name[:1]:
        return False, f"street differs (input {' '.join(a_name)!r} vs matched {' '.join(m_name)!r})"
    if a_num is None and a_name != m_name:
        return False, "input has no house number and street names differ"
    return True, "ok"


def disk_str(lat, lon, k):
    return f"SELECT H3_INT_TO_STRING(value) FROM TABLE(FLATTEN(H3_GRID_DISK(H3_LATLNG_TO_CELL({lat}, {lon}, 7), {k})))"


def disk_int(lat, lon, k):
    return f"SELECT VALUE::NUMBER FROM TABLE(FLATTEN(H3_GRID_DISK(H3_LATLNG_TO_CELL({lat}, {lon}, 7), {k})))"


async def rows_of(data, sql):
    r, ms, vid = await data.call("edgar_data.run_query", q(sql))
    return (r or {}).get("rows", []), ms, vid


def esc(x):
    return (x or "").replace("'", "''")


def haversine_m(lat1, lon1, lat2, lon2):
    from math import asin, cos, radians, sin, sqrt
    p1, p2 = radians(lat1), radians(lat2)
    a = sin((p2 - p1) / 2) ** 2 + cos(p1) * cos(p2) * sin(radians(lon2 - lon1) / 2) ** 2
    return 2 * 6371008.8 * asin(sqrt(a))


async def layers(data, st, lat, lon):
    """Large-footprint evidence (>= 100,000 sq ft) within 1,500 m of a point, per layer, with receipts."""
    pt = f"TO_GEOGRAPHY('POINT({lon} {lat})')"
    ms_rows, _, vid_ms = await rows_of(data, f"""SELECT ROUND(ST_AREA(GEOM_GEOG)*10.7639) SQFT,
        ROUND(ST_DISTANCE(CENTROID_GEOG,{pt})) M
        FROM SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT
        WHERE STATE='{st}' AND H3_R7 IN ({disk_int(lat, lon, 2)})
        AND ST_DWITHIN(CENTROID_GEOG,{pt},1500) AND ST_AREA(GEOM_GEOG)*10.7639 >= 100000
        ORDER BY SQFT DESC LIMIT 40""")
    ov_rows, _, vid_ov = await rows_of(data, f"""SELECT ROUND(ST_AREA(GEOM_GEOG)*10.7639) SQFT,
        ROUND(ST_DISTANCE(CENTROID_GEOG,{pt})) M
        FROM SOURCE.OVERTURE_MAPS_FEATURES_HOT
        WHERE LICENSE_CLASS='odbl' AND DATASET='buildings' AND STATE='US-{st}'
        AND H3_R7 IN ({disk_str(lat, lon, 2)}) AND ST_DWITHIN(CENTROID_GEOG,{pt},1500)
        AND ST_AREA(GEOM_GEOG)*10.7639 >= 100000 ORDER BY SQFT DESC LIMIT 40""")
    fe_rows, _, vid_fe = await rows_of(data, f"""SELECT ROUND(SQFEET) SQFT,
        ROUND(ST_DISTANCE(CENTROID_GEOG,{pt})) M
        FROM SOURCE.FEMA_USA_STRUCTURES_HOT
        WHERE STATE='{st}' AND H3_R7 IN ({disk_str(lat, lon, 2)})
        AND ST_DWITHIN(CENTROID_GEOG,{pt},1500) AND SQFEET >= 100000 ORDER BY SQFT DESC LIMIT 40""")

    def summ(rows):
        return {"n_ge_100k": len(rows), "sqft_sum": sum(x["SQFT"] or 0 for x in rows),
                "largest_sqft": max([x["SQFT"] or 0 for x in rows] or [0]),
                "nearest_m": min(x["M"] for x in rows) if rows else None}

    return {"microsoft_2026_07": {**summ(ms_rows), "vid": vid_ms}, "overture": {**summ(ov_rows), "vid": vid_ov},
            "fema_2023": {**summ(fe_rows), "vid": vid_fe}}


async def site(s, geo, data):
    key, st = s["key"], s.get("place_state")
    city = (s.get("place_candidates") or [None])[0]
    street = (s.get("addresses") or [None])[0]
    out = {"site": key, "epoch_address": s.get("epoch_address"), "org": s.get("org"), "mw": s.get("mw"),
           "input": {"street": street, "city": city, "state": st}, "arms": {}}
    anchor = None
    # ---- Arm A
    if street and city and st:
        q_addr = f"{street}, {city}, {st}"
        r, ms, vid = await geo.call("geo.resolve_address_to_tract", {"address": q_addr})
        a = {"query": q_addr, "vid": vid, "ms": ms}
        if isinstance(r, dict) and r.get("ok"):
            ok, why = guard(street, r["matched_address"])
            a.update({"match_quality": r["match_quality"], "matched": r["matched_address"], "tract": r["tract_geoid"],
                      "lat": r["coordinates"]["y"], "lon": r["coordinates"]["x"], "guard_ok": ok, "guard_reason": why,
                      "grade": ("exact_verified" if r["match_quality"] == "exact" else "relaxed_verified") if ok
                      else "false_" + r["match_quality"]})
            if ok:
                anchor = {"lat": a["lat"], "lon": a["lon"], "by": "A", "tract": a["tract"], "label": a["matched"]}
        else:
            a.update({"grade": "no_match",
                      "match_quality": (r or {}).get("match_quality") if isinstance(r, dict) else None})
        out["arms"]["A"] = a
    else:
        out["arms"]["A"] = {"grade": "not_attempted", "why": "no parseable street+city+state"}
    # ---- Arm B: Foursquare address / owner-name lookup
    b = {"grade": "none"}
    if st and city:
        num = (toks(street) or [""])[0] if street else ""
        core = " ".join([x for x in toks(street)[1:] if x not in DIRS][:2]) if street else ""
        rows = []
        if street and num.isdigit() and core:
            rows, ms, vid = await rows_of(data, f"""SELECT NAME, ADDRESS, LOCALITY, LATITUDE, LONGITUDE, H3_R8, RELEASE_DT
                FROM SOURCE.FOURSQUARE_OS_PLACES_HOT WHERE REGION='{esc(st)}' AND LOCALITY ILIKE '{esc(city)}'
                AND ADDRESS ILIKE '{esc(num)} %' AND DATE_CLOSED IS NULL LIMIT 40""")
            rows = [x for x in rows if all(w in " ".join(toks(x["ADDRESS"])) for w in core.split())]
            b.update({"addr_query": f"{num} {core} @ {city}, {st}", "vid_addr": vid, "n_addr_hits": len(rows)})
        if rows:
            best = next((x for x in rows
                         if s.get("org") and s["org"].lower().split()[0] in (x["NAME"] or "").lower()), rows[0])
            b.update({"grade": "poi_address",
                      "poi": {k: best[k] for k in ("NAME", "ADDRESS", "LOCALITY", "LATITUDE", "LONGITUDE")},
                      "poi_release": best["RELEASE_DT"]})
            if anchor is None:
                anchor = {"lat": best["LATITUDE"], "lon": best["LONGITUDE"], "by": "B",
                          "label": f"{best['NAME']}, {best['ADDRESS']}"}
        elif s.get("org"):
            org = s["org"].split()[0]
            rows, ms, vid = await rows_of(data, f"""SELECT NAME, ADDRESS, LOCALITY, LATITUDE, LONGITUDE
                FROM SOURCE.FOURSQUARE_OS_PLACES_HOT WHERE REGION='{esc(st)}' AND LOCALITY ILIKE '{esc(city)}'
                AND NAME ILIKE '%{esc(org)}%'
                AND (NAME ILIKE '%data cent%' OR FSQ_CATEGORY_LABELS_JSON ILIKE '%data cent%')
                AND DATE_CLOSED IS NULL LIMIT 10""")
            b.update({"name_query": f"{org} data center @ {city}, {st}", "vid_name": vid, "n_name_hits": len(rows)})
            if rows:
                b["grade"] = "poi_name_only"
                b["candidates"] = [{k: x[k] for k in ("NAME", "ADDRESS", "LATITUDE", "LONGITUDE")} for x in rows[:5]]
    out["arms"]["B"] = b
    # ---- Arm C
    c = {"grade": "unresolved" if anchor is None else "anchor_only"}
    if anchor is None and city and st:
        c["grade"] = "city_level"
    if anchor:
        c["anchor_by"] = anchor["by"]
        c.update(await layers(data, st, anchor["lat"], anchor["lon"]))
        if c.get("microsoft_2026_07", {}).get("n_ge_100k") or c.get("overture", {}).get("n_ge_100k"):
            c["grade"] = "ranked_candidates"
        elif c.get("fema_2023", {}).get("n_ge_100k"):
            c["grade"] = "ranked_candidates_fema_only"
    out["arms"]["C"] = c
    out["anchor"] = anchor
    # ---- optional scoring against an independent official point (see notes/truth_*.md)
    tp = s.get("truth_point")
    if tp:
        t = {"truth_lat": tp["lat"], "truth_lon": tp["lon"], "truth_source": tp["source"],
             "anchor_distance_m": (round(haversine_m(anchor["lat"], anchor["lon"], tp["lat"], tp["lon"]))
                                   if anchor else None)}
        t["layers_at_truth"] = await layers(data, st, tp["lat"], tp["lon"])
        out["truth"] = t
    return out


async def main(sp, rp):
    sites = json.load(open(sp))
    f = open(rp, "w")
    t0 = time.time()
    async with Mcp("cmdrvl-geo") as geo, Mcp("cmdrvl-data") as data:
        for i, s in enumerate(sites, 1):
            try:
                r = await site(s, geo, data)
            except Exception as e:  # one bad site never stops the backlog
                r = {"site": s["key"], "error": repr(e)[:300]}
            f.write(json.dumps(r) + "\n")
            f.flush()
            a = r.get("arms", {})
            print(f"[{i:>3}/{len(sites)} {time.time() - t0:6.0f}s] {s['key'][:34]:<34} A={a.get('A', {}).get('grade')} "
                  f"B={a.get('B', {}).get('grade')} C={a.get('C', {}).get('grade')}", flush=True)


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1], sys.argv[2]))
