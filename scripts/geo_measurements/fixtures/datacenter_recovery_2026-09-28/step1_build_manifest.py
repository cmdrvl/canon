"""Build the analyst-facing case manifest for Step 1 of the data-center recovery protocol.

Reads step1_selection.json (the deterministic draw) and the pinned Epoch CSV, and writes case_manifest.json:
the frozen question, grain, as-of date, supplied observations, permissible sources, spatial scope and resource
budget for the development case and for each holdout role (primary plus two ordered reserves).
Nothing here is a label. Labels live in a separate, sealed evaluation artifact referenced only by hash.

Usage: python3 step1_build_manifest.py <epoch data_centers.csv> < step1_selection.json > case_manifest.json
"""
import csv
import hashlib
import json
import re
import sys
from pathlib import Path

epoch_path = Path(sys.argv[1])
sel = json.load(sys.stdin)
epoch_bytes = epoch_path.read_bytes()
if hashlib.sha256(epoch_bytes).hexdigest() != sel["epoch_data_centers_csv_sha256"]:
    raise SystemExit("Epoch CSV does not match the pinned digest in the selection")
rows = {r["Name"]: r for r in csv.DictReader(epoch_bytes.decode("utf-8-sig").splitlines())}

# Fields supplied as observations. Source-link fields are never supplied: they can point at evaluation documents.
KEEP = ["Name", "Owner", "Users", "Current power (MW)", "Current total capital cost (2025 USD billions)", "Current chip types",
        "Investors", "Construction companies", "Energy companies", "Country", "Address"]
NEVER = ["Selected Sources", "Calculations sheet", "Project"]
clean = lambda v: re.sub(r" #\w+", "", v or "").strip()


def observations(name, role):
    r = rows[name]
    obs = {k: clean(r[k]) for k in KEEP if r.get(k, "").strip()}
    withheld = list(NEVER)
    if role == "H3":
        for k in ("Name", "Address", "Users"):
            obs.pop(k, None)
            withheld.append(k)
        m = re.search(r",\s*([A-Z]{2})\b", r["Address"] or "")
        obs["Region"] = {"AL": "Alabama", "VA": "Virginia", "TX": "Texas", "NE": "Nebraska"}.get(m.group(1), m.group(1)) if m else None
    return obs, withheld


COMMON = {
    "as_of": "2026-09-28",
    "target_relationship": "site association (which candidate parcels/buildings, if any, are supported as the site the observations describe). Complete extent is NOT requested.",
    "permissible_sources": [
        "EDGAR_DB.SOURCE Foursquare OS Places (release 2026-08-11), Overture buildings (2026-07), Microsoft GlobalML footprints (2026-07-24), FEMA USA Structures (2023-10-03), each read-only under its query contract",
        "GDELT news tables in EDGAR_DB.DBT_WRANGLING_NEWS (as landed)",
        "Local parcel files only where the case says so",
        "canon geo capabilities/plan/run/inspect"],
    "prohibited_as_evidence": [
        "State, county or municipal air-permit and other regulatory or planning records that name the facility (reserved for the sealed label)",
        "Epoch AI source links or calculation sheets, the operator's own pages, and any repository document under docs/geo_datacenter_site_test_2026-09-28/",
        "Anything read from the sealed evaluation artifact"],
    "resource_budget": {"warehouse_queries_max": 60, "rows_per_query_max": 5000, "wall_clock_minutes_max": 30,
                        "note": "aggregate in SQL rather than returning capped row prefixes; report gaps instead of claiming completeness"},
    "outputs_requested": ["supported association (or none) with uncertainty", "what remains ambiguous or unsupported", "evidence trail with receipts", "next observation that would settle it"],
}

cases = []
# Development case: chosen deliberately, not drawn.
r = rows["Microsoft SAT14"]
obs, withheld = observations("Microsoft SAT14", "DEV")
cases.append({
    "case_id": "DEV-1", "role": "development", "selection": "deliberate (see step1_preregistration.md)", "site": "Microsoft SAT14",
    "question": "Which parcel(s) in Bexar County, Texas are supported as the site that these observations call 'Microsoft SAT14'? State the supported association, what remains ambiguous, and what evidence would settle it.",
    "requested_grain": "parcel", "supplied_observations": obs, "supplied_observations_withheld_fields": withheld,
    "address_role": "The address string is an operator/aggregator claim under test, not a trusted point.",
    "spatial_scope_meaning": "Region = Bexar County, TX. No point is supplied as trusted. Retrieval is centered only on anchors the analyst derives from permissible sources, never on the evaluation label.",
    "parcel_files": "TxGIO 2025 Bexar county parcel file is permissible ONLY if the sealed label is independent of county appraisal records; otherwise it is excluded and this is recorded at sealing.",
    **COMMON})
roles = {"H1": ("H1_known_address_not_placeable", "known address, region rough",
                "Which building(s) or parcel(s) at or near the stated address are supported as the site that these observations call '{site}'? Give the supported association, what remains ambiguous, and what would settle it.",
                "campus (buildings or parcels within the supported footprint)",
                "The address string is an operator/aggregator claim under test, not a trusted point. Region = the city named in the address and its surrounding county."),
         "H2": ("H2_no_address_region_and_attributes", "region and attributes, no supplied point",
                "Where is the facility these observations call '{site}'? Identify the supported location and buildings, if any, from region and attribute evidence alone.",
                "campus (buildings within the supported footprint; multi-building extent reported separately, not required)",
                "No address or coordinates exist in the supplied observations. Region = what the site name and owner imply; the analyst must state the region assumed and why."),
         "H3": ("H3_same_owner_same_state_alternatives", "constructed ambiguity",
                "Which single site, if any, do these observations identify? If they do not identify one, say which alternatives remain and what evidence would separate them.",
                "site (a named facility), not buildings",
                "Region = the stated state. The supplied observations deliberately omit the site name, city and address so that at least one other same-owner site in the state remains plausible; this is a constructed-ambiguity case.")}
for code, (pool, kind, q, grain, scope) in roles.items():
    for i, name in enumerate(sel["selected_primary_then_two_reserves"][pool]):
        obs, withheld = observations(name, code)
        cases.append({
            "case_id": f"{code}-{'P' if i == 0 else f'R{i}'}", "role": kind, "priority": "primary" if i == 0 else f"reserve_{i}",
            "selection": f"drawn: {pool}, seed {sel['seed']}", "site": name if code != "H3" else "(withheld from analyst; see sealed evaluation reference)",
            "question": q.format(site=name),
            "requested_grain": grain, "supplied_observations": obs, "supplied_observations_withheld_fields": withheld,
            "spatial_scope_meaning": scope, **COMMON})
    # the analyst-facing manifest must not leak the H3 site name; keep the public mapping only in the selection file

print(json.dumps({
    "suite": "canon geo data-center recovery, diagnostic suite (not a representative accuracy sample)",
    "pre_registered": "2026-09-28, before any label was sourced",
    "selection_seed": sel["seed"], "epoch_snapshot_sha256": sel["epoch_data_centers_csv_sha256"],
    "sealed_evaluation_reference": {"status": "pending", "location": "outside this repository; only sha256 commitments are recorded here"},
    "cases": cases}, indent=2))
