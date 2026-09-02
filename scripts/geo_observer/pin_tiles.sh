#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/geo_observer/pin_tiles.sh --source <profile|path> --vintage <year> [--windows <json>] [--out <dir>] [--z <zoom>]

Network acquisition is outside Canon's deterministic runtime. This script
fetches live source bytes and writes only retained pin manifests under the
requested pins directory.
USAGE
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
source_arg=""
vintage=""
windows=""
out_dir="$repo_root/scripts/geo_observer/pins"
zoom=16

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      source_arg="${2:-}"
      shift 2
      ;;
    --vintage)
      vintage="${2:-}"
      shift 2
      ;;
    --windows)
      windows="${2:-}"
      shift 2
      ;;
    --out)
      out_dir="${2:-}"
      shift 2
      ;;
    --z)
      zoom="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$source_arg" || -z "$vintage" ]]; then
  usage
  exit 2
fi

if [[ "$source_arg" == */* || "$source_arg" == *.json ]]; then
  source_profile="$source_arg"
else
  source_profile="$repo_root/scripts/geo_observer/sources/${source_arg}.json"
fi

if [[ ! -f "$source_profile" ]]; then
  echo "source profile not found: $source_profile" >&2
  exit 2
fi

mkdir -p "$out_dir"
tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required command not found: $1" >&2
    exit 2
  fi
}

need curl
need jq
need python3

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
  echo "no BLAKE3 helper found; set CANON_BLAKE3_BIN or run the geo observer tests once" >&2
  exit 2
}

profile_id="$(jq -r '.source_profile_id' "$source_profile")"
license_id="$(jq -r '.license.license_id' "$source_profile")"
license_path="$(jq -r '.license.license_text_path' "$source_profile")"
if [[ "$license_path" != /* ]]; then
  license_path="$repo_root/$license_path"
fi
license_text_blake3="$(blake3_file "$license_path")"
range_supported="$(jq -r '.range_requests_supported' "$source_profile")"
source_dataset="$(jq -r --argjson year "$vintage" '.vintages[] | select(.year == $year) | .source_dataset' "$source_profile")"
tile_template="$(jq -r --argjson year "$vintage" '.vintages[] | select(.year == $year) | .tile_url_template' "$source_profile")"
start_day="$(jq -r --argjson year "$vintage" '.vintages[] | select(.year == $year) | .flight_start_day' "$source_profile")"
end_day="$(jq -r --argjson year "$vintage" '.vintages[] | select(.year == $year) | .flight_end_day' "$source_profile")"

if [[ -z "$source_dataset" || "$source_dataset" == "null" ]]; then
  echo "vintage not found in source profile: $vintage" >&2
  exit 2
fi

tile_list="$tmp_dir/tiles.jsonl"
python3 - "$source_profile" "${windows:-}" "$zoom" > "$tile_list" <<'PY'
import json
import math
import sys
from pathlib import Path

source = json.loads(Path(sys.argv[1]).read_text())
windows_path = sys.argv[2]
zoom = int(sys.argv[3])
tiles = []

def lonlat_to_tile(lon, lat, z):
    lat = max(min(lat, 85.05112878), -85.05112878)
    n = 1 << z
    x = math.floor((lon + 180.0) / 360.0 * n)
    lat_rad = math.radians(lat)
    y = math.floor((1.0 - math.asinh(math.tan(lat_rad)) / math.pi) / 2.0 * n)
    return {"z": z, "x": int(x), "y": int(y)}

if windows_path:
    windows = json.loads(Path(windows_path).read_text())
    if "tiles" in windows:
        tiles.extend(windows["tiles"])
    elif "subjects" in windows:
        for subject in windows["subjects"]:
            window = subject.get("window", {})
            bbox = window.get("bbox_wgs84_e7")
            if not bbox:
                continue
            lon = (bbox["xmin_e7"] + bbox["xmax_e7"]) / 20_000_000.0
            lat = (bbox["ymin_e7"] + bbox["ymax_e7"]) / 20_000_000.0
            tile = lonlat_to_tile(lon, lat, zoom)
            tile["window_id"] = subject["subject_id"]
            tiles.append(tile)

if not tiles:
    tiles.extend(source["default_tiles"])

seen = set()
for tile in sorted(tiles, key=lambda item: (item["z"], item["x"], item["y"], item.get("window_id", ""))):
    key = (int(tile["z"]), int(tile["x"]), int(tile["y"]))
    if key in seen:
        continue
    seen.add(key)
    print(json.dumps({"z": key[0], "x": key[1], "y": key[2]}, sort_keys=True))
PY

rows_jsonl="$tmp_dir/rows.jsonl"
while IFS= read -r tile; do
  z="$(jq -r '.z' <<<"$tile")"
  x="$(jq -r '.x' <<<"$tile")"
  y="$(jq -r '.y' <<<"$tile")"
  url="${tile_template//\{z\}/$z}"
  url="${url//\{x\}/$x}"
  url="${url//\{y\}/$y}"
  headers="$tmp_dir/headers.$z.$x.$y"
  bytes="$tmp_dir/tile.$z.$x.$y"
  curl -fsSIL "$url" > "$headers"
  etag="$(awk 'tolower($1)=="etag:" {value=$0; sub(/^[^:]*:[[:space:]]*/, "", value); gsub(/\\r|\"/, "", value); print value}' "$headers" | tail -1)"
  if [[ "$range_supported" == "true" ]]; then
    content_length="$(awk 'tolower($1)=="content-length:" {value=$2; gsub(/\\r/, "", value); print value}' "$headers" | tail -1)"
    curl -fsSL --range "0-$((content_length - 1))" "$url" --output "$bytes"
    byte_range="[0,$((content_length - 1))]"
  else
    curl -fsSL "$url" --output "$bytes"
    byte_range="null"
  fi
  tile_blake3="$(blake3_file "$bytes")"
  jq -n \
    --arg url "$url" \
    --arg etag "$etag" \
    --arg blake3 "$tile_blake3" \
    --arg license_id "$license_id" \
    --arg license_text_blake3 "$license_text_blake3" \
    --arg source_dataset "$source_dataset" \
    --argjson byte_range "$byte_range" \
    --argjson start_day "$start_day" \
    --argjson end_day "$end_day" \
    '{
      url: $url,
      byte_range: $byte_range,
      etag: (if $etag == "" then null else $etag end),
      blake3: $blake3,
      vintage: { start_day: $start_day, end_day: $end_day },
      license_id: $license_id,
      license_text_blake3: $license_text_blake3,
      source_dataset: $source_dataset
    }' >> "$rows_jsonl"
done < "$tile_list"

out_path="$out_dir/nyc_ortho_${vintage}.pins.json"
jq -s \
  --arg source_profile_id "$profile_id" \
  '{version: "canon_geo_image_tile_pin.v0", source_profile_id: $source_profile_id, rows: .}' \
  "$rows_jsonl" > "$out_path"
echo "$out_path"
