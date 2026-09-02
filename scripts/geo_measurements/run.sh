#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s --geography ID --tier ID [--dry-run] [--receipts FILE] [--out DIR] [--manifest FILE] [--repo-root DIR]\n' "${0##*/}" >&2
  printf 'Validates the pinned Canon Geo measurement manifest and classifies supplied result bundles.\n' >&2
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
manifest="$script_dir/manifest.json"
geography=""
tier=""
dry_run=0
receipts=""
out_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --geography)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      geography="$2"
      shift 2
      ;;
    --tier)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      tier="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    --receipts)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      receipts="$2"
      shift 2
      ;;
    --out|--work-dir)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      out_dir="$2"
      shift 2
      ;;
    --manifest)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      manifest="$2"
      shift 2
      ;;
    --repo-root)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      repo_root="$2"
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

if [[ -z "$geography" || -z "$tier" ]]; then
  usage
  printf 'Both --geography and --tier are required.\n' >&2
  exit 64
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'geo measurement runner requires jq on PATH\n' >&2
  exit 69
fi

if [[ -z "$out_dir" ]]; then
  out_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-measurements.XXXXXX")"
else
  mkdir -p "$out_dir"
fi

run_log="$out_dir/run.log"
: > "$run_log"

log() {
  printf '%s\n' "$*" >> "$run_log"
}

say_and_log() {
  printf '%s\n' "$*" >&2
  log "$*"
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

measurement_bin() {
  if [[ -n "${CANON_GEO_MEASUREMENTS_BIN:-}" ]]; then
    printf '%s\n' "$CANON_GEO_MEASUREMENTS_BIN"
    return
  fi
  local target_dir
  target_dir="$(cargo metadata --quiet --no-deps --format-version 1 --manifest-path "$repo_root/Cargo.toml" | jq -r '.target_directory')"
  if [[ -z "$target_dir" || "$target_dir" == "null" ]]; then
    printf 'could not resolve Cargo target directory for %s\n' "$repo_root/Cargo.toml" >&2
    exit 70
  fi
  cargo build --quiet --manifest-path "$repo_root/Cargo.toml" --bin canon_geo_measurements
  printf '%s/debug/canon_geo_measurements\n' "$target_dir"
}

manifest_abs="$manifest"
if [[ "$manifest_abs" != /* ]]; then
  manifest_abs="$repo_root/$manifest_abs"
fi
repo_root_abs="$repo_root"
if [[ "$repo_root_abs" != /* ]]; then
  repo_root_abs="$(cd -- "$repo_root_abs" && pwd -P)"
fi

log "start=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
log "geography=$geography"
log "tier=$tier"
log "manifest=$manifest_abs"

if ! jq -e '.version == "canon_geo_measurement_manifest.v0" and (.measurements | type == "array")' "$manifest_abs" >/dev/null; then
  say_and_log "measurement diverged: manifest is not canon_geo_measurement_manifest.v0 with measurements[]"
  exit 4
fi

selected_path="$out_dir/selected_measurements.json"
jq -e --arg geography "$geography" --arg tier "$tier" '
  [.measurements[]
   | select(.geography == $geography and .tier == $tier)
   | {id, sql_path, source_sql_sha256, gate, declared_grain}]
' "$manifest_abs" > "$selected_path"

selected_count="$(jq 'length' "$selected_path")"
if [[ "$selected_count" == "0" ]]; then
  say_and_log "measurement diverged: no manifest entries for geography $geography tier $tier"
  exit 4
fi
log "selected_count=$selected_count"

while IFS=$'\t' read -r entry_id sql_path expected_sha; do
  sql_abs="$sql_path"
  if [[ "$sql_abs" != /* ]]; then
    sql_abs="$repo_root_abs/$sql_path"
  fi
  if [[ ! -f "$sql_abs" ]]; then
    say_and_log "measurement diverged: sql drift $entry_id missing $sql_path"
    exit 4
  fi
  actual_sha="$(sha256_file "$sql_abs")"
  log "entry=$entry_id sql_path=$sql_path expected_source_sql_sha256=$expected_sha actual_source_sql_sha256=$actual_sha"
  if [[ "$actual_sha" != "$expected_sha" ]]; then
    say_and_log "measurement diverged: sql drift $entry_id $sql_path expected $expected_sha actual $actual_sha"
    exit 4
  fi
done < <(jq -r '.measurements[] | [.id, .sql_path, .source_sql_sha256] | @tsv' "$manifest_abs")

jq -c '.[]' "$selected_path" | while IFS= read -r selected; do
  log "selected=$selected"
done

if [[ "$dry_run" == "1" ]]; then
  log "end=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'dry-run ok: %s entries for geography %s tier %s; run log %s\n' "$selected_count" "$geography" "$tier" "$run_log"
  exit 0
fi

if [[ -z "$receipts" ]]; then
  usage
  printf 'Non-dry-run mode requires --receipts FILE.\n' >&2
  exit 64
fi

receipts_abs="$receipts"
if [[ "$receipts_abs" != /* ]]; then
  receipts_abs="$PWD/$receipts_abs"
fi
report_path="$out_dir/report.json"
validator_stderr="$out_dir/validator.stderr"
bin_path="$(measurement_bin)"

set +e
"$bin_path" \
  --repo-root "$repo_root_abs" \
  --manifest "$manifest_abs" \
  --receipts "$receipts_abs" \
  --emit report \
  > "$report_path" \
  2> "$validator_stderr"
validator_status=$?
set -e

if ! jq -e '.version == "canon_geo_measurement_report.v0"' "$report_path" >/dev/null 2>&1; then
  cat "$validator_stderr" >&2
  say_and_log "measurement diverged: validator did not emit a measurement report"
  exit 4
fi

jq -r '
  .measurements[]
  | "entry=\(.measurement_id) status=\(.status) row_count=\(.row_count // "null") classification=\(.proof_attestation // "none") details=\(.details | join("; "))"
' "$report_path" >> "$run_log"

jq -c '
  .measurements[]
  | select(.result_validation == "exact_manifest_rows")
  | {entry: .measurement_id, expected_result_rows: .expected_result_rows, actual_row_count: .row_count}
' "$manifest_abs" >> "$run_log" 2>/dev/null || true

selected_ids="$(jq -c '[.[].id]' "$selected_path")"
receipts_base="$(cd -- "$(dirname -- "$receipts_abs")" && pwd -P)"
jq -r --argjson selected_ids "$selected_ids" '
  .receipts[]
  | select(.measurement_id as $id | $selected_ids | index($id))
  | [.measurement_id, (.result_artifact_path // "")] | @tsv
' "$receipts_abs" | while IFS=$'\t' read -r entry_id artifact_path; do
  if [[ -n "$artifact_path" && -f "$receipts_base/$artifact_path" ]]; then
    cp "$receipts_base/$artifact_path" "$out_dir/$entry_id.result.json"
    log "result_artifact=$entry_id $out_dir/$entry_id.result.json"
  fi
done

diverged_count="$(jq '(.summary.result_mismatch // 0) + (.summary.malformed // 0) + (.summary.missing // 0)' "$report_path")"
snapshot_count="$(jq '.summary.snapshot_moved // 0' "$report_path")"

if [[ "$diverged_count" != "0" ]]; then
  jq -r '
    .measurements[]
    | select(.status == "result_mismatch" or .status == "malformed" or .status == "missing")
    | "measurement diverged: \(.measurement_id): \(.status): \(.details | join("; "))"
  ' "$report_path" | while IFS= read -r line; do say_and_log "$line"; done
  log "end=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  exit 4
fi

if [[ "$snapshot_count" != "0" ]]; then
  jq -r '
    .measurements[]
    | select(.status == "snapshot_moved")
    | "snapshot moved: \(.measurement_id): \(.details | join("; "))"
  ' "$report_path" | while IFS= read -r line; do say_and_log "$line"; done
  log "end=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  exit 3
fi

if [[ "$validator_status" != "0" ]]; then
  cat "$validator_stderr" >&2
  say_and_log "measurement diverged: validator returned $validator_status without a classified mismatch"
  exit 4
fi

log "end=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
printf 'measurements ok: %s entries for geography %s tier %s; report %s; run log %s\n' "$selected_count" "$geography" "$tier" "$report_path" "$run_log"
