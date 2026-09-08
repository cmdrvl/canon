#!/usr/bin/env python3
"""External Canon Geo re-geocode acquisition helper.

This script may call a live provider, but Canon's deterministic receipt
materialization stays in canon_geo_measurements.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.parse
import urllib.request


PROVIDER_ID = "census_geocoder_current"
PROVIDER_VERSION = "benchmark=Public_AR_Current;vintage=Current_Current"
TOOL_ID = "scripts/geo_acquisition/regeocode.py"
TOOL_VERSION = "bd-3p8e.v0"


def main() -> int:
    args = parse_args()
    request = load_json(args.request)
    request_id = required_string(request, "request_id")
    out_dir = args.out_dir
    staging_dir = out_dir / "_acquisition_inputs" / safe_name(request_id)
    staging_dir.mkdir(parents=True, exist_ok=True)

    import_mode = args.provider_response_bytes is not None or args.candidate_rows is not None
    if import_mode:
        if args.proof_class == "live":
            raise SystemExit("retained response import cannot claim live proof")
        if args.provider_response_bytes is None or args.candidate_rows is None:
            raise SystemExit(
                "--provider-response-bytes and --candidate-rows must be supplied together"
            )
        response_path = args.provider_response_bytes
        rows_path = args.candidate_rows
        response_bytes = response_path.read_bytes()
        query_id = f"retained-provider-response:{sha256_hex(response_bytes)}"
        proof_class = args.proof_class or "retained"
    else:
        if args.proof_class == "retained":
            raise SystemExit("--proof-class retained requires retained response bytes")
        if not args.address:
            raise SystemExit("--address is required unless retained response bytes are supplied")
        response_bytes, query_id = fetch_census_response(args.address, args.timeout_seconds)
        response_path = staging_dir / "provider_response.bytes"
        response_path.write_bytes(response_bytes)
        rows = candidate_rows_from_census(response_bytes, args.address, args.point_id)
        rows_path = staging_dir / "candidate_rows.json"
        rows_path.write_bytes(canonical_json_bytes(rows))
        proof_class = args.proof_class or "live"

    retained_receipt_id = args.retained_receipt_id
    if proof_class == "retained" and not retained_receipt_id:
        retained_receipt_id = f"retained-provider-response:{sha256_hex(response_path.read_bytes())}"

    command = [
        str(resolve_measurement_bin(args.measurement_bin, args.repo_root)),
        "materialize-acquisition-receipt",
        "--request",
        str(args.request),
        "--provider-response-bytes",
        str(response_path),
        "--candidate-rows",
        str(rows_path),
        "--out-dir",
        str(out_dir),
        "--proof-class",
        proof_class,
        "--executor-kind",
        "http-service" if not import_mode else "local-file",
        "--executor-id",
        args.executor_id,
        "--executor-version",
        args.executor_version,
        "--tool-id",
        TOOL_ID,
        "--tool-version",
        TOOL_VERSION,
        "--executor-request-id",
        request_id,
        "--executor-query-id",
        query_id,
    ]
    if retained_receipt_id:
        command.extend(["--retained-receipt-id", retained_receipt_id])
    if args.point_id:
        command.extend(["--executor-attempt-id", f"point:{args.point_id}"])
    result = subprocess.run(command, check=False, text=True, capture_output=True)
    if result.stdout:
        sys.stdout.write(result.stdout)
    if result.stderr:
        sys.stderr.write(result.stderr)
    return result.returncode


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fetch or import fresh geocoder bytes, then materialize a Canon Geo acquisition receipt."
    )
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--address")
    parser.add_argument("--point-id")
    parser.add_argument("--provider-response-bytes", type=Path)
    parser.add_argument("--candidate-rows", type=Path)
    parser.add_argument("--proof-class", choices=["live", "retained"])
    parser.add_argument("--retained-receipt-id")
    parser.add_argument("--measurement-bin", type=Path)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--executor-id", default=PROVIDER_ID)
    parser.add_argument("--executor-version", default=PROVIDER_VERSION)
    parser.add_argument("--timeout-seconds", type=float, default=20.0)
    return parser.parse_args()


def fetch_census_response(address: str, timeout_seconds: float) -> tuple[bytes, str]:
    params = urllib.parse.urlencode(
        {
            "address": address,
            "benchmark": "Public_AR_Current",
            "vintage": "Current_Current",
            "format": "json",
        }
    )
    url = (
        "https://geocoding.geo.census.gov/geocoder/geographies/onelineaddress"
        f"?{params}"
    )
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "canon-geo-regeocode-bd-3p8e/0",
        },
    )
    with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
        return response.read(), f"{PROVIDER_ID}:{sha256_hex(url.encode('utf-8'))}"


def candidate_rows_from_census(
    response_bytes: bytes, address: str, point_id: str | None
) -> list[dict[str, object]]:
    response = json.loads(response_bytes.decode("utf-8"))
    matches = response.get("result", {}).get("addressMatches", [])
    if not isinstance(matches, list):
        raise SystemExit("Census geocoder response did not contain result.addressMatches[]")
    rows: list[dict[str, object]] = []
    for index, match in enumerate(matches, start=1):
        if not isinstance(match, dict):
            continue
        coordinates = match.get("coordinates", {})
        lon = coordinates.get("x") if isinstance(coordinates, dict) else None
        lat = coordinates.get("y") if isinstance(coordinates, dict) else None
        row: dict[str, object] = {
            "provider_id": PROVIDER_ID,
            "provider_version": PROVIDER_VERSION,
            "candidate_rank": index,
            "input_address_sha256": sha256_hex(address.encode("utf-8")),
            "matched_address": match.get("matchedAddress"),
            "match_type": match.get("tigerLine", {}).get("side")
            if isinstance(match.get("tigerLine"), dict)
            else None,
            "accuracy_type": "census_geocoder_current",
        }
        if point_id:
            row["point_id"] = point_id
        if lon is not None:
            row["lon_e7"] = int(round(float(lon) * 10_000_000))
        if lat is not None:
            row["lat_e7"] = int(round(float(lat) * 10_000_000))
        tract_geoid = census_tract_geoid(match)
        if tract_geoid:
            row["tract_geoid"] = tract_geoid
        rows.append({key: value for key, value in row.items() if value is not None})
    return rows


def census_tract_geoid(match: dict[str, object]) -> str | None:
    geographies = match.get("geographies")
    if not isinstance(geographies, dict):
        return None
    tracts = geographies.get("Census Tracts")
    if not isinstance(tracts, list) or not tracts:
        return None
    first = tracts[0]
    if not isinstance(first, dict):
        return None
    geoid = first.get("GEOID")
    return geoid if isinstance(geoid, str) and geoid else None


def resolve_measurement_bin(measurement_bin: Path | None, repo_root: Path) -> Path:
    candidates = []
    if measurement_bin is not None:
        candidates.append(measurement_bin)
    env_bin = os.environ.get("CANON_GEO_MEASUREMENTS_BIN")
    if env_bin:
        candidates.append(Path(env_bin))
    candidates.append(repo_root / "target" / "debug" / "canon_geo_measurements")
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    raise SystemExit(
        "canon_geo_measurements binary not found; pass --measurement-bin or set CANON_GEO_MEASUREMENTS_BIN"
    )


def load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise SystemExit(f"{path} must contain a JSON object")
    return value


def required_string(value: dict[str, object], field: str) -> str:
    field_value = value.get(field)
    if not isinstance(field_value, str) or not field_value:
        raise SystemExit(f"request is missing nonempty {field}")
    return field_value


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_hex(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def safe_name(value: str) -> str:
    return "".join(char if char.isalnum() or char in "._-" else "_" for char in value)


if __name__ == "__main__":
    raise SystemExit(main())
