#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s --work-dir DIR [--live-receipts FILE --geography ID --tier ID]\n' "${0##*/}" >&2
  printf 'Runs the CI-safe measurement dry run and the focused geo measurement tests.\n' >&2
}

work_dir=""
live_receipts=""
live_geography=""
live_tier=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      work_dir="$2"
      shift 2
      ;;
    --live-receipts)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      live_receipts="$2"
      shift 2
      ;;
    --geography)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      live_geography="$2"
      shift 2
      ;;
    --tier)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      live_tier="$2"
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
  printf '--work-dir is required.\n' >&2
  exit 64
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
mkdir -p "$work_dir"

bash "$repo_root/scripts/geo_measurements/run.sh" \
  --geography nyc \
  --tier nyc_full \
  --dry-run \
  --out "$work_dir/dry-run"

cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_measurement_runner -- --nocapture

if [[ -n "$live_receipts" ]]; then
  if [[ -z "$live_geography" || -z "$live_tier" ]]; then
    usage
    printf '--live-receipts requires --geography and --tier for the live bundle.\n' >&2
    exit 64
  fi
  bash "$repo_root/scripts/geo_measurements/run.sh" \
    --geography "$live_geography" \
    --tier "$live_tier" \
    --receipts "$live_receipts" \
    --out "$work_dir/live"
  shasum -a 256 "$work_dir/live/run.log" > "$work_dir/live/run.log.sha256"
fi
