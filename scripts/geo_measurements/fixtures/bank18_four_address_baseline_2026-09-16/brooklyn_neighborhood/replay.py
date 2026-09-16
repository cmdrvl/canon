# /// script
# requires-python = ">=3.11"
# dependencies = ["blake3==1.0.8", "h3==4.3.1", "shapely==2.1.2"]
# ///
"""Offline, operator-assembled Brooklyn measurement; not a production source adapter.

Usage: uv run replay.py --canon /absolute/path/to/canon --out /new/workspace
No network acquisition occurs here. Source rows are retained beside this script.
Shapely is used only to validate and display retained WKT, never to admit constraints.
Record hashes bind the acquired query-row projection, not the provider's original archive.
"""
import argparse
import hashlib
import html
import json
import math
from pathlib import Path
import shutil
import subprocess

import blake3
import h3
from shapely import wkt


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()


def digest(value):
    return blake3.blake3(compact(value)).hexdigest()


def render_view(out, source, parcels, buildings, pad, matched_rows, summaries):
    """Approximate local metric display; no image inference or solver geometry."""
    lon, lat = source["subject"]["longitude"], source["subject"]["latitude"]
    dx, dy = 111320 * math.cos(math.radians(lat)), 111320
    extent = [wkt.loads(r["GEOM_WKT"]).bounds for r in parcels + buildings]
    xmin = min((b[0] - lon) * dx for b in extent) - 20
    xmax = max((b[2] - lon) * dx for b in extent) + 20
    ymin = min((b[1] - lat) * dy for b in extent) - 20
    ymax = max((b[3] - lat) * dy for b in extent) + 20
    scale = min(800 / (xmax - xmin), 800 / (ymax - ymin))

    def xy(x, y):
        return ((x - lon) * dx - xmin) * scale + 30, (ymax - (y - lat) * dy) * scale + 30

    def path_for(row):
        geom = wkt.loads(row["GEOM_WKT"])
        polygons = list(geom.geoms) if geom.geom_type == "MultiPolygon" else [geom]
        parts = []
        for polygon in polygons:
            for ring in [polygon.exterior, *polygon.interiors]:
                points = [xy(x, y) for x, y in ring.coords]
                parts.append("M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in points) + " Z")
        return " ".join(parts)

    target_parcels = {r["BBL_KEY"] for r in matched_rows}
    target_bins = {r["BIN_KEY"] for r in matched_rows}
    layers = []
    for name, rows in (("parcels", parcels), ("buildings", buildings)):
        elements = []
        for row in rows:
            ident = row["BBL"].removesuffix(".0") if name == "parcels" else row["BIN"]
            matched = ident in (target_parcels if name == "parcels" else target_bins)
            detail = (f"BBL {ident} | {row['ADDRESS']} | Assessor buildings: {row['NUMBLDGS']}"
                      if name == "parcels" else f"BIN {ident} | source BBL {row['BBL']} | {row['DISTANCE_M']:.1f} m from anchor")
            elements.append(f'<path class="{name} {"matched" if matched else ""}" d="{path_for(row)}"><title>{html.escape(detail)}</title></path>')
        layers.append(f'<g id="{name}">' + "".join(elements) + '</g>')
    cx, cy = xy(lon, lat)
    circle = f'<g id="search"><circle cx="{cx:.2f}" cy="{cy:.2f}" r="{150*scale:.2f}" fill="none" stroke="#8762a8" stroke-dasharray="7 6"/><circle cx="{cx:.2f}" cy="{cy:.2f}" r="6" fill="#cf6819" stroke="white" stroke-width="2"><title>Census geocode: discovery anchor; outside the linked parcel and footprint</title></circle><text x="{cx+10:.2f}" y="{cy+4:.2f}" font-size="12" fill="#9d4b0c">Census point</text></g>'
    scale_bar = f'<path d="M35,842 h{50*scale:.2f}" stroke="#243747" stroke-width="3"/><text x="35" y="832" font-size="12">50 m (approx.)</text><text x="820" y="35" font-size="18">N ↑</text>'
    svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 860 870" role="img" aria-label="Brooklyn neighborhood parcels and buildings">' + ''.join(layers) + circle + scale_bar + '</svg>'
    frontages = [r for r in pad if r["BBL_KEY"] in target_parcels]
    frontage_html = ''.join(f'<li>{html.escape(r["LOW_HOUSE_NUMBER_DISPLAY"])}–{html.escape(r["HIGH_HOUSE_NUMBER_DISPLAY"])} {html.escape(r["STREET_NAME"].title())}</li>' for r in frontages)
    table_rows = ''.join('<tr><td>' + label + '</td><td>' + ', '.join(s["hard_forced"]["parcels"]) + '</td><td>' + ', '.join(s["hard_forced"]["buildings"]) + '</td><td>' + s["status"] + '</td></tr>'
                         for label, s in zip(("Neighborhood only", "+ native address → parcel", "+ explicit PAD building link"), summaries))
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Brooklyn · Canon Geo neighborhood walkthrough</title>
<style>body{font:16px/1.5 system-ui,sans-serif;color:#233442;background:#f5f6f4;margin:0}main{max-width:1280px;margin:auto;padding:28px}h1{font-size:30px;margin:3px 0}h2{font-size:19px}p{max-width:850px}.small{font-size:13px;color:#546572}.layout{display:grid;grid-template-columns:minmax(480px,1.8fr) minmax(280px,1fr);gap:24px}.map,.notes{background:white;border:1px solid #d8dfdf;border-radius:8px;padding:16px}.map svg{width:100%;height:auto}.parcels{fill:#f6f3e9;stroke:#a5aaa2;stroke-width:.8;fill-rule:evenodd}.buildings{fill:#b5cad7;stroke:#6e8b9d;stroke-width:.6;fill-rule:evenodd}.parcels.matched{fill:#d7efe2;stroke:#23825c;stroke-width:2}.buildings.matched{fill:#42a77f;stroke:#176e49;stroke-width:1.6}path:hover{stroke:#d4781a;stroke-width:2}label{display:inline-block;margin-right:14px;font-size:14px}table{width:100%;border-collapse:collapse;background:white;font-size:14px}td,th{padding:10px;text-align:left;border-bottom:1px solid #dbe1df}.finding{border-left:4px solid #23825c;padding-left:14px}li{margin:6px 0}@media(max-width:850px){.layout{grid-template-columns:1fr}main{padding:16px}h1{font-size:24px}}</style>
<main><div class="small">CANON GEO · RETAINED REAL SOURCE SNAPSHOTS · 16 SEPTEMBER 2026</div><h1>2207 Albemarle Road, Brooklyn</h1><p>One address question, a neighborhood of evidence. Green marks the source-supported parcel and building; blue and beige show the surrounding candidates retained in the solve.</p>
<div class="layout"><section class="map"><div><label><input type="checkbox" checked data-layer="parcels">Parcels</label><label><input type="checkbox" checked data-layer="buildings">Footprints</label><label><input type="checkbox" checked data-layer="search">150 m discovery circle</label></div>''' + svg + '''<div class="small">Hover over a polygon for its source identifier. Approximate display coordinates; no satellite imagery. The circle is a declared acquisition scope, not a proven address-accuracy bound.</div></section>
<aside class="notes"><h2>1 · Gather the neighborhood</h2><p>92 parcels · 98 active footprints · 131 address/frontage rows.</p><p class="small">91 parcels intersect the discovery circle. One additional parcel was acquired because a nearby footprint references it. Five footprints extend beyond the circle but belong to acquired parcels.</p>
<h2>2 · Read the address evidence</h2><p>The native PAD membership command matches 2207 to the odd-number range 2207–2231. That row names BBL 3051090025 and BIN 3117371.</p><p>The same parcel has three frontages:</p><ul>''' + frontage_html + '''</ul>
<h2>3 · Solve the constraints together</h2><div class="finding"><strong>The parcel and building are required members in the final declared solve.</strong><p>The complete property set remains ambiguous: no admitted evidence says these are its only members.</p></div>
<h2>What stays uncertain</h2><p class="small">Ten frontage records have unsupported house-number representations and remain unrepresented. Geocode accuracy, independent truth reach, historical validity and complete property extent are unproven. The building observation is manually assembled from the same PAD row; it is not independent corroboration.</p></aside></div>
<h2>What changes as evidence enters</h2><table><thead><tr><th>Stage</th><th>Required parcel</th><th>Required building</th><th>Whole-set result</th></tr></thead><tbody>''' + table_rows + '''</tbody></table>
<p><strong>Execution limit:</strong> each nine-stage plan completes seven stages, including solving and explanation. Separation rejects the empty prospective-observation list, leaving next-evidence selection blocked. The overall run reports FAILED; the retained solve results above remain inspectable.</p>
<p class="small">“Required” means present in every feasible composition relative to the supplied universe and admitted source statements. Parcel/building incidences come from source BBL references; this run does not admit a new geometric or imagery constraint. Source dates: PAD 26B (May 1), MapPLUTO 26v2 (August 1), NYC footprints (August 9). Sources: NYC Department of City Planning and NYC Office of Technology and Innovation; their source terms and limitations apply. No 2019 collateral boundary claim.</p></main>
<script>document.querySelectorAll('[data-layer]').forEach(c=>c.addEventListener('change',()=>document.getElementById(c.dataset.layer).style.display=c.checked?'':'none'));</script></html>'''
    (out / "walkthrough.html").write_text(page)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canon", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        raise ValueError("Use a new empty output directory; retained runs are immutable")
    canon = out / "canon-measurement-bin"
    shutil.copy2(args.canon.resolve(), canon)
    journal = []

    def write(name, value):
        path = out / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
        return path

    def run(name, *argv):
        result = subprocess.run([str(canon), *map(str, argv)], capture_output=True)
        path = out / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(result.stdout)
        journal.append({"argv": list(map(str, argv)), "exit_code": result.returncode,
                        "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                        "stderr": result.stderr.decode(), "output": name})
        write("commands.json", journal)
        value = json.loads(result.stdout)
        if result.returncode not in (0, 1):
            raise RuntimeError(f"{argv}: {value}")
        return value

    source = json.loads((root / "source_records.json").read_text())
    expansion = json.loads((root / "boundary_expansion.json").read_text())
    transform = json.loads((root / "transform_receipt.json").read_text())
    assert hashlib.sha256(transform["payload_text"].encode()).hexdigest() == transform["payload_sha256"]
    parcels = source["parcels"]["rows"] + expansion["parcels"]["rows"]
    buildings = source["buildings"]["rows"]
    pad = source["pad"]["rows"] + expansion["pad"]["rows"]
    parcel_ids = sorted(r["BBL"].removesuffix(".0") for r in parcels)
    assert len(set(parcel_ids)) == len(parcels)
    assert len({r["BIN"] for r in buildings}) == len(buildings)
    assert all(r["BBL"] in parcel_ids for r in buildings)
    assert len(source["parcels"]["rows"]) == source["parcels"]["row_count"]
    assert len(buildings) == source["buildings"]["row_count"]
    assert len(source["pad"]["rows"]) == source["pad"]["row_count"]
    recovered = source["buildings"]["geometry_recovery"]["rows"][0]
    recovered_row = next(r for r in buildings if r["BIN"] == recovered["BIN"])
    assert hashlib.sha256(recovered_row["GEOM_WKT"].encode()).hexdigest() == recovered["WKT_SHA256"]
    for row in parcels + buildings:
        assert not wkt.loads(row["GEOM_WKT"]).is_empty
    # The source writer hashes unrounded derived WKB, then publishes display WKT
    # rounded to nine decimals (cmdrvl-curves/.../nyc_dcp_pluto/writer.py:845).
    # Those are different byte representations. Preserve the source WKB pin and
    # separately hash the exact WKT bytes we actually acquired.
    geometry_bindings = [{"bbl": row["BBL"], "source_wkb_sha256": row["GEOM_WGS84_SHA256"],
                          "retained_display_wkt_sha256": hashlib.sha256(row["GEOM_WKT"].encode()).hexdigest()}
                         for row in parcels]
    write("geometry_bindings.json", {"bindings": geometry_bindings,
          "source_wkb_readback": "not performed; source-provided digest retained as provenance",
          "retained_geometry": "complete source display WKT rounded to nine decimals; map and discovery only"})

    # Preserve the grammar's field order because Canon hashes the typed member bytes.
    members, bindings, excluded = [], [], []
    member_rows = {}
    for row in pad:
        lo, hi = row["LOW_HOUSE_NUMBER_DISPLAY"], row["HIGH_HOUSE_NUMBER_DISPLAY"]
        if not lo or not hi or not lo.isdigit() or not hi.isdigit():
            excluded.append({"source_row_number": row["SOURCE_ROW_NUMBER"],
                             "reason": "house-number representation unsupported by this bounded adapter",
                             "low": lo, "high": hi, "street": row["STREET_NAME"]})
            continue
        words = row["STREET_NAME_NORMALIZED"].lower().split()
        street = {}
        if words[0] in ("east", "west", "north", "south"):
            street["pre_direction"] = words.pop(0)
        if words[-1] not in ("road", "avenue", "street", "terrace"):
            raise ValueError(f"Unimplemented street grammar: {words}")
        suffix = words.pop()
        street["name"] = [{"kind": "ordinal", "value": int(word)} if word.isdigit()
                          else {"kind": "literal", "value": word} for word in words]
        street["suffix"] = suffix
        house = ({"kind": "discrete", "value": int(lo)} if lo == hi else
                 {"kind": "range", "start": int(lo), "end": int(hi),
                  "parity": {"0": "any", "1": "odd", "2": "even"}[row["PARITY"]],
                  "operator": "dash", "asserted_numbers": []})
        member = {"member_id": f"nyc.pad:26B:{row['SOURCE_ROW_NUMBER']}",
                  "lot_id": row["BBL_KEY"], "house": house, "street": street}
        record = {"source_record_id": f"SOURCE.NYC_DCP_PAD_ADDRESS_HOT:26B:{row['SOURCE_ROW_NUMBER']}",
                  "source_vintage": "26B/2026-05-01", "record_blake3": digest(row)}
        members.append(member)
        bindings.append({"member_id": member["member_id"], "normalized_member_blake3": digest(member),
                         "source_record": record})
        member_rows[member["member_id"]] = row
    request = json.loads((root.parent / "nyc_address_request.json").read_text())
    request["address_set"]["members"] = members
    request["bridge_request"]["member_source_records"] = bindings
    address_path = write("address_request.json", request)
    address = run("address_bundle.json", "geo", "materialize-address-evidence", "--request", address_path)
    observation = address["bridge"]["observation"]
    matched_ids = sorted({mid for r in address["bridge"]["readings"] for mid in r["source_supported_member_ids"]})
    matched_rows = [member_rows[mid] for mid in matched_ids]
    supported_bins = sorted({r["BIN_KEY"] for r in matched_rows if r["BIN_KEY"]})
    assert all(bin_id in {r["BIN"] for r in buildings} for bin_id in supported_bins)
    write("adapter_accounting.json", {"input_frontage_rows": len(pad), "typed_members": len(members),
          "unrepresented_rows": excluded, "matched_source_rows": matched_rows,
          "normalization": "MapPLUTO terminal .0 removed only for BBL field; PAD street tokens/number parity explicitly typed",
          "building_bridge": "Manually assembled existential BIN inclusion from the very same matched PAD records; not independent evidence or a shipped PAD-to-building adapter",
          "geometry_role": "Source BBL-to-BIN references supply incidences; WKT displayed, no geometric predicate admitted as hard evidence"})

    profile = {"version": "canon_geo_composition_profile.v0", "selection_level": "parcel"}
    region = {"geography_id": "brooklyn.albemarle.discovery150m.parent-closure",
              "geography_kind": "bounded_source_snapshot",
              "description": "All v3 parcels intersecting a 150 m circle around the retained Census anchor, plus parents of nearby active footprints. 92 parcels; independent truth reach unverified."}
    budget = {"version": "canon_geo_resource_budget.v0", "budget_id": "budget.brooklyn.neighborhood",
              "deterministic_bounds": [{"semantic_id": "budget.max_" + counter, "counter": counter,
                  "value": value, "unit": unit, "origin": "caller_declared", "action": "report_budget_fallback"}
                  for counter, value, unit in [("bytes", 20000000, "byte"), ("rows", 10000, "row"),
                    ("cells", 127, "cell"), ("candidates", 1000, "candidate"), ("variables", 1000, "variable"),
                    ("states", 1000000, "state"), ("models", 16, "model"), ("operations", 10000000, "operation")]],
              "telemetry": []}
    question = {"version": "canon_geo_question.v0", "question_id": "question.brooklyn.neighborhood",
                "subject_bindings": [{"role": "target", "binding_class": "operator_label", "value": source["subject"]["address"]}],
                "bounded_geography": region,
                "requested_grains": [{"entity_level": "parcel", "required_evidence_classes": ["parcel_geometry"],
                                       "optional_evidence_classes": ["address_set", "building_footprint"]}],
                "query_as_of": None, "requested_claim_classes": ["stable_identity"],
                "presentation_limits": [],
                "abstention_policy": {"unsupported_grain": "report_unsupported", "unresolved_residual": "report_residual",
                                      "budget_fallback": "report_residual"},
                "resource_budget_ref": budget["budget_id"]}
    release = {"release_id": "mappluto.26v2.brooklyn.retained-subset",
               "release_digest": "blake3:" + digest(parcels)}
    inventory_rows = {"version": "canon_geo_warehouse_rows.v0", "profile": profile,
                      "parcel_rows": [{"parcel_id": p} for p in parcel_ids],
                      "building_parcel_rows": [{"building_id": b["BIN"], "parcel_id": b["BBL"]} for b in buildings],
                      "contracts": [], "evidence_rows": [], "max_assignments": 1000000, "max_materialized_models": 16}
    inventory_rows_path = write("inventory_warehouse_rows.json", inventory_rows)
    inventory = {"version": "canon_geo_regional_inventory.v1", "inventory_id": "inventory.brooklyn.neighborhood",
                 "region": region, "sources": [{"source_instance_id": "nyc.mappluto.v3", "release": release,
                 "temporal_scope": {"release_time": {"utc_day": "2026-08-01", "semantic_id": "source.release_date",
                   "unit": "utc_day", "origin": "source_release"}},
                 "lineage_ids": ["NYC_DCP_MAPPLUTO:26v2"],
                 "native_scope": {"kind": "native_entity", "entity_level": "parcel", "identity_participation": "stable_alias"},
                 "evidence_classes": ["parcel_geometry"],
                 "coverage": {"coverage_id": region["geography_id"], "region": region, "predicate": region["description"]},
                 "local_state": {"state": "available", "local_ref": {"artifact_id": "retained.brooklyn.parcels",
                   "contract_version": "canon_geo_warehouse_rows.v0", "content_hash": "blake3:" + blake3.blake3(inventory_rows_path.read_bytes()).hexdigest(),
                   "media_type": "application/json"}},
                 "geometry": {"geometry_contract_version": "nyc_dcp_mappluto_geometry_evidence.v3",
                   "coordinate_reference_system": "EPSG:4326", "transform_id": parcels[0]["TRANSFORM_DEFINITION_ID"],
                   "transform_digest": "blake3:" + blake3.blake3(transform["payload_text"].encode()).hexdigest(),
                   "numeric_error_bounds": [{"semantic_id": "transform.declared_accuracy", "value": 2000,
                       "unit": "millimeter", "origin": "source_release"}]},
                 "license_class": "public_redistributable", "egress_class": "shareable",
                 "estimates": [{"semantic_id": "source.rows", "value": len(parcels), "unit": "row", "origin": "local_artifact"}]}],
                 "discovery_gaps": []}
    run("capabilities.json", "geo", "capabilities")
    for name, value in (("profile", profile), ("question", question), ("inventory", inventory), ("budget", budget)):
        write(name + ".json", value)
    plan = run("plan.json", "geo", "plan", "--question", out / "question.json", "--capabilities", out / "capabilities.json",
               "--inventory", out / "inventory.json", "--profile", out / "profile.json", "--budget", out / "budget.json")
    if plan["status"] != "planned":
        raise ValueError(f"Plan did not reach planned: {plan}")
    tile_source = {"source_instance_id": "nyc.mappluto.v3", "release": release,
                   "native_scope": inventory["sources"][0]["native_scope"], "inventory_ref": plan["inventory_ref"]}
    home = {"version": "canon_geo_home_cell_rows.v1", "coordinate_crs": "EPSG:4326", "coordinate_decimal_places": 9,
            "h3_resolution": 9, "stability_radius_fixed": 1000, "max_rows": 1000,
            "rows": [{"source": tile_source, "feature_id": row["BBL"].removesuffix(".0"),
                "source_record_id": f"mappluto:26v2:bk:{row['SOURCE_ROW_NUMBER']}",
                "geometry_sha256": row["GEOM_WGS84_SHA256"], "representative_point_method": "centroid_of_derived_wgs84_geometry",
                "longitude": f"{row['CENTROID_LON']:.9f}", "latitude": f"{row['CENTROID_LAT']:.9f}",
                "transform_execution_id": row["TRANSFORM_EXECUTION_ID"], "transform_definition_id": row["TRANSFORM_DEFINITION_ID"]}
                for row in parcels]}
    home_path = write("home_cell_rows.json", home)
    assignment = run("home_cell_assignment.json", "geo", "materialize-home-cells", "--rows", home_path)
    center = h3.latlng_to_cell(source["subject"]["latitude"], source["subject"]["longitude"], 9)
    cells = [feature["home_cell"] for feature in assignment["tile_work_features"]]
    halo = max(1, max(h3.grid_distance(center, cell) for cell in cells))
    write("tile_request.json", {"version": "canon_geo_tile_work_request.v1", "center_cell": center, "halo_k": halo,
          "features": assignment["tile_work_features"], "max_features": 1000, "max_work_cells": 127})
    write("separation_inputs.json", {"version": "canon_geo_separation_inputs.v0", "prospective": []})
    write("next_evidence_inputs.json", {"version": "canon_geo_next_evidence_inputs.v0", "candidates": [],
          "budget": budget, "budget_spent": {}})
    contract = {"id": observation["contract_id"], "version": "1.0.0", "source_dataset": "NYC_DCP_PAD_ADDRESS_HOT",
                "source_release": "26B/2026-05-01", "source_lineage_ids": ["NYC_DCP_PAD:26B"],
                "method_id": "canon_native_pad_existential_parcel_membership", "method_version": "1.0.0",
                "claim_role": "stable_identity_anchor",
                "basis": {"kind": "logical_relaxation", "invariant_id": "asserted_PAD_address_membership_implies_at_least_one_named_parcel_in_source_relative_composition"}}
    building_contract = {**contract, "id": "rho.brooklyn.pad.bin", "method_id": "operator_adapter_matched_PAD_BIN_existential_membership",
                         "basis": {"kind": "logical_relaxation", "invariant_id": "matched_PAD_row_with_nonnull_BIN_asserts_building_membership_not_complete_set"}}
    building_observation = {**observation, "id": "obs.brooklyn.pad.bin", "contract_id": building_contract["id"],
                            "observation": {"kind": "existential_membership", "members": [{"level": "building", "id": b} for b in supported_bins]}}
    summaries = []
    for label, observations, contracts in [("00_neighborhood", [], []),
            ("01_address_parcel", [observation], [contract]),
            ("02_address_building", [observation, building_observation], [contract, building_contract])]:
        rows = {"version": "canon_geo_warehouse_rows.v0", "profile": profile,
                "parcel_rows": [{"parcel_id": p} for p in parcel_ids],
                "building_parcel_rows": [{"building_id": b["BIN"], "parcel_id": b["BBL"]} for b in buildings],
                "contracts": contracts, "evidence_rows": [{"observation_id": obs["id"], "contract_id": obs["contract_id"],
                    "source_record": record, "observation": obs["observation"]} for obs in observations for record in obs["source_records"]],
                "max_assignments": 1000000, "max_materialized_models": 16}
        rows_path = write(label + "/warehouse_rows.json", rows)
        (out / label / "workspace").mkdir()
        args_run = ["geo", "run", "--plan", out / "plan.json", "--work-dir", out / label / "workspace"]
        for node, binding, path in [("home_cells", "rows", home_path), ("section", "request", out / "tile_request.json"),
                  ("materialize_evidence", "rows", rows_path), ("separation", "request", out / "separation_inputs.json"),
                  ("next_evidence", "request", out / "next_evidence_inputs.json")]:
            args_run.extend(["--input", f"geo.parcel.{node}:{binding}={path}"])
        result = run(label + "/run.json", *args_run)
        inspect = run(label + "/inspect.json", "geo", "inspect", "--run", out / label / "workspace", "--recommend-next")
        # Locate actual solver output by contract, not an assumed workspace layout.
        solves = []
        for path in (out / label / "workspace").rglob("*.json"):
            value = json.loads(path.read_text())
            if isinstance(value, dict) and value.get("version") == "canon_geo_composition.v0":
                solves.append((path, value))
        assert len(solves) == 1, solves
        solve_path, solve = solves[0]
        write(label + "/solve.json", solve)
        report = result["project_run_report"]
        assert "geo.parcel.solve" in report["executed_nodes"]
        assert report["failed_nodes"] == ["geo.parcel.separation"]
        failure = json.loads((out / label / "workspace/.canon/geo-run/receipts/geo_parcel_separation.json").read_text())
        assert "require at least one prospective observation" in failure["failure_message"]
        target_components = [component for component in solve["factorization"]
                             if any(v["level"] == "parcel" and v["id"] in {r["BBL_KEY"] for r in matched_rows}
                                    for v in component["variables"])]
        summaries.append({"stage": label, "status": solve["status"], "summary": solve["summary"],
                          "hard_forced": solve["hard_forced"], "source_observations": len(observations),
                          "target_components": target_components,
                          "failure_message": failure["failure_message"],
                          "solve_path": str(solve_path.relative_to(out)), "run_status": result["status"],
                          "executed_nodes": result["project_run_report"]["executed_nodes"],
                          "failed_nodes": result["project_run_report"]["failed_nodes"],
                          "blocked_nodes": result["project_run_report"]["blocked_nodes"]})
        print(json.dumps(summaries[-1]), flush=True)
    write("measurement.json", {"version": "brooklyn_neighborhood_measurement.v0", "proof_class": "retained_real_source_snapshot_execution",
          "canon_binary_sha256": hashlib.sha256(canon.read_bytes()).hexdigest(), "parcels": len(parcels), "footprints": len(buildings),
          "frontage_rows": len(pad), "typed_address_members": len(members), "unrepresented_address_rows": excluded,
          "halo_k": halo, "stages": summaries, "limits": ["No independent truth reach proof", "No historical or current physical-validity proof",
          "No property-set completeness assertion", "Same PAD row supplies parcel and building observations; not independent corroboration",
          "Manual source adapter and node bindings; not automated Evidence Machine integration", "No geometry-derived hard constraint",
          "Empty prospective-observation inputs are rejected by separation; seven stages complete, separation fails and next_evidence is blocked; no end-to-end success or recommendation claimed"]})
    render_view(out, source, parcels, buildings, pad, matched_rows, summaries)


if __name__ == "__main__":
    main()
