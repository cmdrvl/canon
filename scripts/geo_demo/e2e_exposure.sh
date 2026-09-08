#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s --work-dir DIR\n' "${0##*/}" >&2
  printf 'Runs the Canon Geo event-exposure join over fixture ledger geometry and a pinned advisory.\n' >&2
}

work_dir=""
canon_bin="${CANON_BIN:-target/debug/canon}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      if [[ $# -lt 2 ]]; then
        usage
        exit 64
      fi
      work_dir="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 64
      ;;
  esac
done

if [[ -z "$work_dir" ]]; then
  usage
  exit 64
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'e2e_exposure requires jq on PATH\n' >&2
  exit 69
fi

if [[ ! -x "$canon_bin" ]]; then
  printf 'missing executable %s; build target/debug/canon or set CANON_BIN before running this harness\n' "$canon_bin" >&2
  exit 70
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

log_digest() {
  local label="$1"
  local file="$2"
  local digest
  digest="$(blake3_file "$file")"
  printf '%s blake3:%s %s\n' "$label" "$digest" "$file" >>"$log"
}

blake3_file() {
  local path="$1"
  if command -v b3sum >/dev/null 2>&1; then
    b3sum "$path" | awk '{print $1}'
    return
  fi
  if [[ -n "${CANON_BLAKE3_BIN:-}" && -x "${CANON_BLAKE3_BIN:-}" ]]; then
    "$CANON_BLAKE3_BIN" "$path" | awk '{print $1}'
    return
  fi
  local cached="/tmp/canon-blake3-helper-bd-2g4z/blake3_file"
  if [[ -x "$cached" ]]; then
    "$cached" "$path" | awk '{print $1}'
    return
  fi
  local rlib
  rlib="$(find "$repo_root/target/debug/deps" -maxdepth 1 -name 'libblake3-*.rlib' -print -quit 2>/dev/null || true)"
  if [[ -n "$rlib" ]] && command -v rustc >/dev/null 2>&1; then
    local helper="$work_dir/blake3_file"
    cat >"$work_dir/blake3_file.rs" <<'RS'
use std::{env, fs, process};

fn main() {
    let Some(path) = env::args().nth(1) else {
        process::exit(2);
    };
    let Ok(bytes) = fs::read(path) else {
        process::exit(2);
    };
    println!("{}", blake3::hash(&bytes).to_hex());
}
RS
    rustc --edition 2024 -L dependency="$repo_root/target/debug/deps" --extern blake3="$rlib" "$work_dir/blake3_file.rs" -o "$helper"
    "$helper" "$path"
    return
  fi
  printf 'no BLAKE3 helper found; set CANON_BLAKE3_BIN or run the geo observer tests once\n' >&2
  exit 2
}

run_json() {
  local out="$1"
  shift
  {
    printf '+'
    printf ' %q' "$@"
    printf ' > %q\n' "$out"
  } | tee -a "$log"
  set +e
  "$@" >"$out" 2>>"$log"
  local status=$?
  set -e
  printf 'exit_code=%s\n' "$status" | tee -a "$log"
  return "$status"
}

assert_refusal_code() {
  local out="$1"
  local expected="$2"
  jq -e --arg code "$expected" '.outcome == "REFUSAL" and .refusal.detail.geo_exposure_error_code == $code' "$out" >/dev/null
}

ledger="$work_dir/ledger.json"
geometry="$work_dir/building-geometry.json"
advisory="$work_dir/advisory.json"
archive="$work_dir/archive.json"
exposure="$work_dir/exposure.json"
missing_archive="$work_dir/archive-missing-pin.json"
stale_refusal="$work_dir/stale-refusal.json"
point_geometry="$work_dir/point-geometry.json"
geometry_refusal="$work_dir/geometry-refusal.json"

cp "$repo_root/tests/fixtures/geo/case4_building_geometry.json" "$geometry"
cp "$repo_root/tests/fixtures/geo/advisory_synthetic_adv12.json" "$advisory"

jq -n --arg version "canon_geo_collateral_ledger.v0" '
{
  version: $version,
  proof_class: "fixture",
  rows: [
    {
      version: $version,
      accession: "fixture.d3.exposure",
      deal_id: "fixture.d3.exposure.deal",
      loan_id: "fixture.d3.exposure.loan",
      reach: "full",
      parcel_set: ["1004540041", "1004540042", "1004540043", "1004540044", "1004540045", "1004540046"],
      building_set: ["1006494", "1006495", "1006496", "1006497", "1006498", "1006499"],
      deed_ids: [],
      truth_plane: "gate_v2_historical",
      claim_class: "collateral_composition",
      residual_model_count: 1,
      count_exact: true,
      backbone_complete: true,
      source_release_pins: [
        {
          source_dataset: "fixture.case4.geometry",
          source_release: "2026-09-02",
          blake3: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
      ],
      composition_blake3: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      evidence_blake3: "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      ambiguous_parcel_set: [],
      ambiguous_building_set: [],
      property_refs: [],
      composition_status: "resolved"
    }
  ],
  rollups: [
    {
      deal_id: "fixture.d3.exposure.deal",
      accession: "fixture.d3.exposure",
      rows: 1,
      truth_planes: {
        gate_v2_historical: {
          resolved: 1,
          ambiguous: 0,
          conflict: 0,
          reach_none: 0,
          budget_fallback: 0
        }
      }
    }
  ]
}' >"$ledger"

jq '{ source_blake3s: .source_blake3s, advisories: [{ storm_id: .storm_id, advisory_number: .advisory_number, source_blake3s: .source_blake3s }] }' "$advisory" >"$archive"

log_digest ledger "$ledger"
log_digest geometry "$geometry"
log_digest advisory "$advisory"
log_digest archive "$archive"

jq -r '
  "ring vertices",
  (.wind_radii[] | "\(.knots) \(.ring.exterior.vertices | length)")
' "$advisory" >>"$log"

run_json "$exposure" "$canon_bin" geo ledger exposure \
  --ledger "$ledger" \
  --advisory "$advisory" \
  --geometry "$geometry" \
  --archive "$archive"

jq -e '
  (.version == "canon_geo_event_exposure.v0")
  and (([.exposed[] | select(.building_id == "1006494" and .knots_band == 64)] | length) == 1)
  and (([.exposed[] | select(.building_id == "1006495" and .knots_band == 50)] | length) == 1)
  and (([.exposed[] | select(.building_id == "1006496" and .knots_band == 34)] | length) == 1)
  and (([.exposed[] | select(.building_id == "1006497")] | length) == 0)
  and (([.exposed[] | select(.building_id == "1006499")] | length) == 0)
  and ((.advisory.source_blake3s | length) > 0)
  and (.buildings_without_geometry == [])
' "$exposure" >/dev/null

jq -r '
  "exposed_by_band",
  (.exposed | group_by(.knots_band)[]? | "\(.[0].knots_band) \(length)"),
  "buildings_without_geometry \(.buildings_without_geometry | length)"
' "$exposure" >>"$log"

jq '.source_blake3s = [] | .advisories[].source_blake3s = []' "$archive" >"$missing_archive"
if run_json "$stale_refusal" "$canon_bin" geo ledger exposure \
  --ledger "$ledger" \
  --advisory "$advisory" \
  --geometry "$geometry" \
  --archive "$missing_archive"; then
  printf 'expected missing advisory archive pin to refuse\n' >&2
  exit 1
fi
assert_refusal_code "$stale_refusal" exposure_advisory_stale

jq '{ frame_id, buildings: (.buildings | with_entries(.value = { "kind": "point", "coordinate": { "x": 10000, "y": 10000 } })) }' "$geometry" >"$point_geometry"
if run_json "$geometry_refusal" "$canon_bin" geo ledger exposure \
  --ledger "$ledger" \
  --advisory "$advisory" \
  --geometry "$point_geometry" \
  --archive "$archive"; then
  printf 'expected point geometry to refuse when every building lacks a polygon\n' >&2
  exit 1
fi
assert_refusal_code "$geometry_refusal" exposure_geometry_missing

printf 'e2e_exposure wrote %s\n' "$exposure"
