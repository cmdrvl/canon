"""Step 1 case selection for the data-center recovery protocol (docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md).

Deterministic and pre-registered. The development case is chosen deliberately (see step1_preregistration.md). The three
holdouts are DRAWN so nobody can pick a convenient site:
  1. Pool = Epoch AI US rows (77 in the 2026-09-28 snapshot; sha256 pinned in the output) minus every site examined in detail
     during the field test (EXAMINED below).
  2. Role pools use Epoch and run-01 facts only, never labels:
       H1 known address the geocoder could not place: street address present, run-01 arm A in {no_match, false_exact}
       H2 region-and-attributes: Epoch has NO street address at all, capacity >= 50 MW (no trusted point exists to supply)
       H3 ambiguity: the same owner has at least one other Epoch US site in the same state, capacity >= 50 MW, so genuine
          alternatives exist
  3. Within a role, order by sha256(SEED | role | site name). First = primary; next two = ordered reserves, used only if the
     adjudicator cannot source an independent label (recorded, never skipped silently).
  4. Roles are filled H1, H2, H3; a site chosen or reserved for one role leaves later pools.

Usage: python3 step1_case_selection.py <repo-root> <epoch data_centers.csv> > step1_selection.json
The Epoch CSV is CC BY 4.0 and is not copied into this repo; its sha256 is recorded in the output and in
docs/geo_datacenter_site_test_2026-09-28/data/source_snapshots.json.
"""
import csv
import hashlib
import json
import re
import sys
from pathlib import Path

SEED = "canon-geo-data-center-recovery-2026-09-28"
root = Path(sys.argv[1])
epoch_path = Path(sys.argv[2])
results = root / "docs/geo_datacenter_site_test_2026-09-28/runs/01_epoch_arms_abc/results_epoch69_v2.jsonl"

# Sites examined in detail during the field test (labels sourced, join-tested, guard-tested, or discussed in the brief).
EXAMINED = {
    "Google Pryor (North)", "Meta Los Lunas", "Microsoft SAT14", "Microsoft SAT40", "Vantage TX1",
    "OpenAI Stargate Abilene", "OpenAI Stargate Shackelford", "Crusoe Abilene Expansion", "Anthropic Barber Lake",
    "Google New Albany", "AWS New Albany", "Meta Prometheus", "Google Columbus", "Google Lancaster",
    "Anthropic-Amazon New Carlisle", "Colossus 1", "Colossus 2", "Meta Montgomery", "Meta Sarpy",
    "Microsoft Fairwater Atlanta", "Amazon Ridgeland", "Amazon Madison Mega Site", "Core42 Lake Mariner",
    "Anthropic Lake Mariner", "Meta Cheyenne", "Meta Kuna", "Google Lincoln", "Meta Eagle Mountain", "QTS Eagle Mountain",
    "Google Red Oak", "Microsoft-Nebius New Jersey", "Google Fort Wayne", "Meta Hyperion", "Google Council Bluffs (East)",
    "CoreWeave Chester VA", "QTS Richmond 1", "OpenAI Stargate New Mexico", "OpenAI Stargate Wisconsin",
}


def key(role, name):
    return hashlib.sha256(f"{SEED}|{role}|{name}".encode()).hexdigest()


def clean_owner(s):
    return re.sub(r" #\w+", "", s or "").strip()


def state_of(address):
    m = re.search(r",\s*([A-Z]{2})\b", address or "")
    return m.group(1) if m else None


def has_street(address):
    return bool(re.match(r"^\s*\d", address or "")) or bool(re.search(r"\b(rd|road|st|street|ave|avenue|blvd|dr|drive|pkwy|hwy|lane|ln|way)\b", (address or "").lower()))


epoch_bytes = epoch_path.read_bytes()
epoch = [r for r in csv.DictReader(epoch_bytes.decode("utf-8-sig").splitlines()) if r["Country"] == "United States"]
run01 = {}
for l in results.read_text().splitlines():
    r = json.loads(l)
    if "arms" in r:
        run01[r["site"]] = r

sites = []
for r in epoch:
    name = r["Name"]
    if name in EXAMINED:
        continue
    addr = (r["Address"] or "").strip()
    mw = float(r["Current power (MW)"] or 0)
    sites.append({"site": name, "owner": clean_owner(r["Owner"]), "mw": mw, "address": addr, "state": state_of(addr),
                  "street": has_street(addr), "A": (run01.get(name) or {}).get("arms", {}).get("A", {}).get("grade")})

owner_state = {}
for r in epoch:
    st = state_of((r["Address"] or "").strip())
    if st:
        owner_state.setdefault((clean_owner(r["Owner"]), st), []).append(r["Name"])

pools = {
    "H1_known_address_not_placeable": [s for s in sites if s["street"] and s["A"] in ("no_match", "false_exact")],
    "H2_no_address_region_and_attributes": [s for s in sites if not s["address"] and s["mw"] >= 50],
    "H3_same_owner_same_state_alternatives": [s for s in sites if s["state"] and s["mw"] >= 50 and s["owner"]
                                              and len(owner_state.get((s["owner"], s["state"]), [])) >= 2],
}
taken, selected, detail = set(), {}, {}
for role, pool in pools.items():
    ordered = sorted((s for s in pool if s["site"] not in taken), key=lambda s: key(role, s["site"]))
    picks = ordered[:3]
    taken.update(s["site"] for s in picks)
    selected[role] = [p["site"] for p in picks]
    detail[role] = {"pool_size_after_exclusions": len(ordered), "ordered": [
        {"site": s["site"], "hash": key(role, s["site"])[:12], "mw": s["mw"], "owner": s["owner"], "state": s["state"]} for s in ordered]}

print(json.dumps({
    "seed": SEED, "epoch_data_centers_csv_sha256": hashlib.sha256(epoch_bytes).hexdigest(),
    "epoch_us_rows": len(epoch), "results_file": str(results.relative_to(root)),
    "excluded_examined": sorted(EXAMINED), "eligible_after_exclusions": len(sites),
    "role_pool_filters": {
        "H1": "street address present and run-01 arm A grade in {no_match, false_exact}",
        "H2": "Epoch Address field empty and capacity >= 50 MW",
        "H3": "same owner has >= 1 other Epoch US site in the same state; capacity >= 50 MW"},
    "selected_primary_then_two_reserves": selected, "detail": detail}, indent=2))
