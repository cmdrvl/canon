#!/usr/bin/env -S uv run --script --python 3.12
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = ["blake3==1.0.5", "h3==4.3.1", "shapely==2.1.1"]
# ///
"""Offline Hillsborough adapter for the first addressless experiment.

Acquisition is separate. `prepare` reads only the retained regional export and
SEC descriptor selection; `evaluate` reads independently withheld truth after a
fresh Canon process has published its run. The experiment uses `canon geo run`;
the isolated truth-feasibility probe uses native `canon geo solve`.
"""
import argparse
import gzip
import hashlib
import json
import subprocess
import time
from pathlib import Path

import blake3
import h3
from shapely.geometry import Polygon
from shapely.ops import unary_union


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return blake3.blake3(value).hexdigest()


def read(path):
    if path.exists():
        return path.read_bytes()
    return gzip.decompress(path.with_suffix(path.suffix + ".gz").read_bytes())


def write(root, name, value):
    (root / name).write_bytes(encoded(value) + b"\n")


def command(args, root, name, *words):
    result = subprocess.run([str(args.canon), *map(str, words)], capture_output=True)
    (root / name).write_bytes(result.stdout)
    (root / (name + ".stderr")).write_bytes(result.stderr)
    if result.returncode not in (0, 1):
        raise RuntimeError(f"Canon failed ({result.returncode}): {result.stdout.decode()} {result.stderr.decode()}")
    return json.loads(result.stdout)


def record(id_, vintage, raw):
    return {"source_record_id": id_, "source_vintage": vintage, "record_blake3": digest(raw)}


def prepare(args):
    root = args.work_dir
    root.mkdir(parents=True, exist_ok=True)
    (root / "run").mkdir(exist_ok=True)
    raw = read(args.sources / "candidate-parcels.json")
    data = json.loads(raw)
    assert not data.get("exceededTransferLimit") and "error" not in data
    features = sorted(data["features"], key=lambda f: f["attributes"]["folio"])
    selection_bytes = read(args.sources / "selection.json")
    selection = json.loads(selection_bytes)
    claim = selection["selected"]
    chosen = min(selection["cohort"], key=lambda r: hashlib.sha256((selection["selection_seed"] + r["property_name"]).encode()).hexdigest())
    assert chosen == claim, "selection seed/cohort mismatch"
    source_digest = "blake3:" + digest(raw)
    warehouse_inventory = {"version": "canon_geo_warehouse_rows.v0", "profile": {"version": "canon_geo_composition_profile.v0", "selection_level": "parcel"},
        "parcel_rows": [{"parcel_id": f["attributes"]["folio"]} for f in features], "building_parcel_rows": [], "contracts": [], "evidence_rows": [],
        "max_assignments": 5_000_000, "max_materialized_models": 2000}
    write(root, "inventory-rows.json", warehouse_inventory)
    region = {"geography_id": "us.fl.hillsborough.dor03", "geography_kind": "county_assessor_subset", "description": selection["inventory_scope"]}
    release = {"release_id": "retained-" + digest(raw)[:16], "release_digest": source_digest}
    native_scope = {"kind": "native_entity", "entity_level": "parcel", "identity_participation": "stable_alias"}
    limits = {"bytes": 100_000_000, "rows": 10000, "cells": 10000, "candidates": 2000,
              "variables": 2000, "states": 5_000_000, "models": 2000, "operations": 50_000_000}
    budget = {"version": "canon_geo_resource_budget.v0", "budget_id": "addressless.budget",
              "deterministic_bounds": [{"semantic_id": "max_" + k, "counter": k, "value": v, "unit": k,
                "origin": "caller_declared", "action": "report_budget_fallback"} for k, v in limits.items()],
              "telemetry": [{"metric": "wall_time", "unit": "millisecond", "origin": "operator_policy", "semantic_effect": "none"}]}
    question = {"version": "canon_geo_question.v0", "question_id": "addressless.one-property",
        "subject_bindings": [{"role": "target", "binding_class": "operator_label", "value": claim["property_name"]}],
        "bounded_geography": region, "requested_grains": [{"entity_level": "parcel", "required_evidence_classes": ["parcel_geometry"], "optional_evidence_classes": ["asserted_attribute"]}],
        "requested_claim_classes": ["collateral_composition"], "presentation_limits": [],
        "abstention_policy": {"unsupported_grain": "report_unsupported", "unresolved_residual": "report_residual", "budget_fallback": "report_residual"},
        "resource_budget_ref": budget["budget_id"]}
    inventory = {"version": "canon_geo_regional_inventory.v1", "inventory_id": "addressless.regional.inventory", "region": region,
        "sources": [{"source_instance_id": "hillsborough.assessor.parcels", "release": release, "temporal_scope": {},
            "lineage_ids": ["hillsborough-property-appraiser"], "native_scope": native_scope,
            "evidence_classes": ["parcel_geometry", "asserted_attribute"],
            "coverage": {"coverage_id": "county.dor03", "region": region, "predicate": "dor_code LIKE '03%'"},
            "local_state": {"state": "available", "local_ref": {"artifact_id": "retained.regional.inventory-rows", "contract_version": "canon_geo_warehouse_rows.v0", "content_hash": "blake3:" + digest((root / "inventory-rows.json").read_bytes()), "media_type": "application/json"}},
            "geometry": {"geometry_contract_version": "arcgis.wgs84.polygon.v1", "coordinate_reference_system": "EPSG:4326", "transform_id": "server.outSR.4326", "transform_digest": "blake3:" + digest(b"ArcGIS outSR=4326"), "numeric_error_bounds": []},
            "license_class": "public_redistributable", "egress_class": "shareable",
            "estimates": [{"semantic_id": "retained_rows", "value": len(features), "unit": "row", "origin": "source_release"}]}], "discovery_gaps": []}
    profile = {"version": "canon_geo_composition_profile.v0", "selection_level": "parcel"}
    for name, value in [("question", question), ("inventory", inventory), ("profile", profile), ("budget", budget)]:
        write(root, name + ".json", value)
    command(args, root, "capabilities.json", "geo", "capabilities")
    plan = command(args, root, "plan.json", "geo", "plan", "--question", root / "question.json", "--capabilities", root / "capabilities.json", "--inventory", root / "inventory.json", "--profile", root / "profile.json", "--budget", root / "budget.json")
    assert plan["status"] == "planned" and not plan["external_requests"], plan["status"]
    source = {"source_instance_id": inventory["sources"][0]["source_instance_id"], "release": release,
              "native_scope": native_scope, "inventory_ref": plan["inventory_ref"]}
    candidates, home_rows, tile_features = [], [], []
    for feature in features:
        a = feature["attributes"]
        assert a["dor_code"].startswith("03")
        id_ = a["folio"]
        # Polygon centroids are blocking metadata only, never identity evidence.
        polygon = unary_union([Polygon(ring) for ring in feature["geometry"]["rings"]])
        point = polygon.centroid
        lat, lon = f"{point.y:.9f}", f"{point.x:.9f}"
        cell = h3.latlng_to_cell(float(lat), float(lon), 7)
        tile_features.append({"source": source, "feature_id": id_, "home_cell": cell})
        home_rows.append({"source": source, "feature_id": id_, "source_record_id": str(a["objectid"]),
            "geometry_sha256": hashlib.sha256(encoded(feature["geometry"])).hexdigest(), "representative_point_method": "regional-polygon-centroid-blocking-only",
            "longitude": lon, "latitude": lat, "claimed_home_cell": cell})
        # Explicit zero/sentinel policy, frozen before inspecting withheld truth.
        if a["tunits"] is not None and a["tunits"] > 0 and a["tunits"] != int(a["tunits"]):
            raise ValueError(f"Nonintegral unit count on source parcel {id_}")
        attrs = {"property_name": a["dba"].strip() if a["dba"] and a["dba"].strip() else None,
                 "property_type": "multifamily", "unit_count": int(a["tunits"]) if a["tunits"] and a["tunits"] > 0 else None,
                 "year_built": int(a["act"]) if a["act"] and a["act"] > 0 else None}
        candidates.append({"id": id_, "attributes": attrs, "source_records": [record("assessor:" + str(a["objectid"]), release["release_id"], encoded(feature))]})
    assert len({c["id"] for c in candidates}) == len(candidates)
    center = h3.latlng_to_cell(27.95, -82.30, 7)  # County-scale blocking center, fixed without subject location.
    halo = max(h3.grid_distance(center, f["home_cell"]) for f in tile_features)
    write(root, "home.json", {"version": "canon_geo_home_cell_rows.v1", "coordinate_crs": "EPSG:4326", "coordinate_decimal_places": 9,
        "h3_resolution": 7, "stability_radius_fixed": 0, "rows": home_rows, "max_rows": 2000})
    write(root, "section.json", {"version": "canon_geo_tile_work_request.v1", "center_cell": center, "halo_k": halo,
        "features": tile_features, "max_features": 2000, "max_work_cells": 10000})
    policies = []
    for channel in ["property_name", "property_type", "unit_count", "year_built"]:
        policies.append({"channel": channel, "tolerance": selection["year_tolerance"] if channel == "year_built" else selection["unit_tolerance"] if channel == "unit_count" else 0,
            "contract": {"id": "rho.descriptive." + channel, "version": "1", "source_dataset": "retained-assessor-and-disclosure", "source_release": release["release_id"],
                "source_lineage_ids": ["hillsborough-property-appraiser", "nxrt-company-disclosure"],
                "method_id": "declared-record-compatibility-experiment", "method_version": "1", "claim_role": "attribute_observation",
                "basis": {"kind": "logical_relaxation", "invariant_id": "conditional-on-recorded-same-grain-attributes-being-correct-and-comparable"}}})
    request = {"version": "canon_geo_descriptive_asset_request.v0",
        "profile": {"profile_id": "descriptive_asset_single_member_v0", "selection_level": "parcel", "channels": policies},
        "bounded_geography": region, "inventory_source": source,
        "claim": {"claim_id": "nxrt-selected-property", "as_of": "2025-12-31",
            "attributes": {"property_name": claim["property_name"], "property_type": "multifamily", "unit_count": claim["unit_count"], "year_built": None},
            "source_records": [record("nxrt-q4-2025-supplement:property-table:page-28", "2025-12-31", read(args.sources / "nxrt-q4-2025.pdf")),
                record("sec:0001193125-26-065382:ex99.1:property-table", "2025-12-31", read(args.sources / "sec-nxrt-2025q4-ex991.html")),
                record("frozen-selection", "2026-09-17", selection_bytes)]},
        "candidates": candidates, "max_candidates": 2000, "max_assignments": 5_000_000, "max_materialized_models": 2000}
    write(root, "descriptive.json", request)
    # Modeled prospective outcomes, not acquired evidence: an independent
    # source locates the subject among parcels with/without recorded unit counts.
    # Exhaustive binary masks avoid materializing hundreds of allowed sets.
    outcomes = []
    for label, known in [("known_units", True), ("unknown_units", False)]:
        values = [{"id": c["id"], "value": int((c["attributes"]["unit_count"] is not None) != known)} for c in candidates]
        if any(v["value"] == 0 for v in values):
            outcomes.append({"outcome_id": label, "induced": [{"kind": "integer_sum_band", "level": "parcel",
                "measure": {"semantic_id": "prospective.unit-count-presence", "unit": "contradicted_member", "value_origin": "source_asserted"},
                "values": values, "min": 0, "max": 0}]})
    observation_id = "independent-units-record-completeness"
    write(root, "separation.json", {"version": "canon_geo_separation_inputs.v0", "prospective": [
        {"id": observation_id, "contract_id": "prospective.unit-count-inventory-gap", "cost_units": 1, "outcomes": outcomes}]})
    write(root, "next.json", {"version": "canon_geo_next_evidence_inputs.v0", "candidates": [{
        "action_id": "acquire-independent-unit-count-support", "class": "separate_residual", "kind": {"kind": "observe", "payload": observation_id},
        "observation_id": observation_id, "cost_units": 1, "redundant": False, "lineage_ids": []}], "budget": budget})
    args_ = ["geo", "run", "--plan", str(root / "plan.json"), "--work-dir", str(root / "run")]
    for node, binding, filename in [("home_cells", "rows", "home.json"), ("section", "request", "section.json"), ("materialize_evidence", "rows", "descriptive.json"), ("separation", "request", "separation.json"), ("next_evidence", "request", "next.json")]:
        args_ += ["--input", f"geo.parcel.{node}:{binding}={root / filename}"]
    write(root, "run-command.json", [str(args.canon), *args_])
    print(json.dumps({"candidates": len(candidates), "known_units": sum(c["attributes"]["unit_count"] is not None for c in candidates), "halo_k": halo, "next": str(root / "run-command.json")}))


def evaluate(args):
    root = args.work_dir
    # Only this phase opens truth, after the native run is independently readable.
    run = json.loads((root / "run.json").read_bytes())
    solve = json.loads((root / "run/geo/parcel/solve.json").read_bytes())
    request = json.loads((root / "descriptive.json").read_bytes())
    truth = json.loads(read(args.sources / "truth-parcels.json"))
    truth_ids = {r["attributes"]["FOLIO"].replace(".", "").zfill(10) for r in truth["features"]}
    candidate_ids = {c["id"] for c in request["candidates"]}
    compiled = json.loads((root / "run/geo/parcel/compile_evidence.json").read_bytes())
    feasibility = json.loads(json.dumps(compiled["composition_request"]))
    feasibility["hard_constraints"].append({"id": "evaluation-only-withheld-truth", "constraint": {
        "kind": "allowed_sets", "level": "parcel", "sets": [sorted(truth_ids)]}})
    write(root, "truth-feasibility-request.json", feasibility)
    feasible = command(args, root, "truth-feasibility.json", "geo", "solve", "--request", root / "truth-feasibility-request.json") if truth_ids <= candidate_ids else None
    truth_reachable = feasible is not None and feasible["summary"]["residual_model_count_complete"] and feasible["summary"]["residual_model_count"] == 1
    separation = json.loads((root / "run/geo/parcel/separation.json").read_bytes())
    next_evidence = json.loads((root / "run/geo/parcel/next_evidence.json").read_bytes())
    summary = {"property": request["claim"]["attributes"]["property_name"], "proof_class": "retained-public-data-experiment",
        "initial_candidates": len(candidate_ids), "truth_parcel_ids": sorted(truth_ids), "candidate_reach": truth_ids <= candidate_ids,
        "exact_residual": solve["summary"]["residual_model_count_complete"] and not solve["summary"]["residual_model_count_saturated"],
        "final_residual_size": solve["summary"]["residual_model_count"], "withheld_truth_reachable": truth_reachable,
        "forced_identity": solve["status"] == "resolved", "run_status": run["status"],
        "blind_experiment_success": truth_reachable and solve["summary"]["residual_model_count_complete"]
            and not solve["summary"]["residual_model_count_saturated"] and len(candidate_ids) > solve["summary"]["residual_model_count"],
        "identity_claim_status": "unresolved" if truth_reachable else "blocked_failed_truth_reach",
        "evaluation_next_action": "Acquire independent separating evidence" if truth_reachable else
            "Investigate source grain, vintage and attribute comparability; revoke truth-excluding hard assumptions before claiming identity. Do not tune the frozen tolerance to this subject.",
        "truth_attributes_evaluation_only": [{"id": c["id"], "attributes": c["attributes"]} for c in request["candidates"] if c["id"] in truth_ids],
        "admissions": [{"observation_id": a["observation_id"], "disposition": a["disposition"], "generated_ids": a["generated_ids"]} for a in compiled["admissions"]],
        "hard_constraint_count": len(compiled["composition_request"]["hard_constraints"]),
        "soft_preference_count": len(compiled["composition_request"]["soft_preferences"]),
        "solver_summary": solve["summary"], "prospective_separation": separation,
        "next_evidence": next_evidence, "execution": json.loads((root / "execution.json").read_bytes()),
        "resource_budget": json.loads((root / "budget.json").read_bytes()),
        "limits": ["Conditional compatibility of recorded same-grain attributes; no population calibration or independent truth precision claim.",
                   "Regional inventory is restricted to declared DOR 03xx parcels, not all county physical assets.",
                   "Single-parcel hypothesis; complete collateral/building composition is not established.",
                   "Current assessor snapshot versus 2025 disclosure: historical coverage is unverified.",
                   "Claim year built absent; source unit-count sentinels stay unknown; names never harden identity."]}
    write(root, "evaluation.json", summary)
    print(json.dumps(summary, indent=2))


def run(args):
    """Launch the existing project runner once; no alternate scheduler/cache."""
    argv = json.loads((args.work_dir / "run-command.json").read_bytes())
    argv[0] = str(args.canon)
    started = time.monotonic()
    result = subprocess.run(argv, capture_output=True)
    elapsed = time.monotonic() - started
    (args.work_dir / "run.json").write_bytes(result.stdout)
    (args.work_dir / "run.stderr").write_bytes(result.stderr)
    write(args.work_dir, "execution.json", {"argv": argv, "exit_code": result.returncode,
        "elapsed_seconds": elapsed, "binary_blake3": digest(args.canon.read_bytes()),
        "source_manifest_blake3": digest(read(args.sources / "source-manifest.json"))})
    print(json.dumps({"exit_code": result.returncode, "elapsed_seconds": elapsed}))
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stdout.decode() + result.stderr.decode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "run", "evaluate"])
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--canon", type=Path, default=Path("target/debug/canon"))
    options = parser.parse_args()
    options.canon = options.canon.resolve()
    options.work_dir = options.work_dir.resolve()
    {"prepare": prepare, "run": run, "evaluate": evaluate}[options.action](options)
