#!/usr/bin/env bash
set -euo pipefail

# Real-data proof for bd-vdcw.11. The only registry writes are to WORK_DIR;
# the source gold_sets directory and served registries are never modified.

gold_dir="/Users/zac/Source/cmdrvl/cmdrvl-soi/gold_sets"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-bdc-proof.XXXXXX")"
canon_bin="${CANON_BIN:-}"
seed="bd-vdcw.11-day-one-v1"
second_source=""
keep_work=0
owns_work=1

cleanup() {
  if [[ "$owns_work" -eq 1 && "$keep_work" -eq 0 ]]; then
    rm -rf "$work_dir"
  else
    printf 'work_dir=%s\n' "$work_dir" >&2
  fi
}
trap cleanup EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --gold-dir)
      gold_dir="${2:?missing gold directory}"
      shift 2
      ;;
    --work-dir)
      work_dir="${2:?missing work directory}"
      mkdir -p "$work_dir"
      owns_work=0
      shift 2
      ;;
    --canon-bin)
      canon_bin="${2:?missing canon executable}"
      shift 2
      ;;
    --seed)
      seed="${2:?missing selection seed}"
      shift 2
      ;;
    --second-source)
      second_source="${2:?missing second-source JSONL}"
      shift 2
      ;;
    --keep-work)
      keep_work=1
      shift
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$canon_bin" ]]; then
  if [[ -x target/debug/canon ]]; then
    canon_bin="target/debug/canon"
  elif [[ -x target/release/canon ]]; then
    canon_bin="target/release/canon"
  else
    canon_bin="$(command -v canon || true)"
  fi
fi
if [[ -z "$canon_bin" || ! -x "$canon_bin" ]]; then
  printf 'set CANON_BIN or --canon-bin to an existing canon executable\n' >&2
  exit 2
fi
if [[ ! -d "$gold_dir" ]]; then
  printf 'missing gold-set directory: %s\n' "$gold_dir" >&2
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
converter="$script_dir/entity_sessions/bdc_goldsets_to_session.py"
mkdir -p "$work_dir"

run_json() {
  local label="$1"
  shift
  printf '\n[%s] command:' "$label"
  printf ' %q' "$@"
  printf '\n'
  local output
  local status=0
  output=$("$@") || status=$?
  printf '%s\n' "$output"
  RUN_JSON="$output"
  RUN_STATUS="$status"
}

run_expected_json() {
  local label="$1"
  shift
  run_json "$label" "$@"
  if [[ "$RUN_STATUS" -ne 0 ]]; then
    printf '[%s] unexpected exit=%s\n' "$label" "$RUN_STATUS" >&2
    exit "$RUN_STATUS"
  fi
}

run_expected_failure_json() {
  local label="$1"
  shift
  run_json "$label" "$@"
  if [[ "$RUN_STATUS" -eq 0 ]]; then
    printf '[%s] expected a domain failure, got exit=0\n' "$label" >&2
    exit 1
  fi
}

count_goldsets=$(find "$gold_dir" -maxdepth 1 -type f -name '*.goldset.v*.json' | wc -l | tr -d ' ')
printf 'proof_class=real_data_public_command_replay\n'
printf 'selection_seed=%s\n' "$seed"
printf 'goldset_file_denominator=%s\n' "$count_goldsets"
printf 'countermetric=ids_other_than_54_or_cik_resolves_to_another_filer_name\n'

python3 - "$gold_dir" "$work_dir/all-ciks.txt" "$work_dir/day-one-ciks.txt" "$work_dir/selection.json" "$seed" <<'PY'
import hashlib
import json
import pathlib
import sys

gold_dir = pathlib.Path(sys.argv[1])
all_out = pathlib.Path(sys.argv[2])
day_one = pathlib.Path(sys.argv[3])
receipt_out = pathlib.Path(sys.argv[4])
seed = sys.argv[5]
rows = []
for path in sorted(gold_dir.glob("*.goldset.v*.json")):
    value = json.loads(path.read_text(encoding="utf-8"))
    source = value["source"]
    rows.append((source["cik"], source["company"], path.name))
ciks = sorted({row[0] for row in rows})
all_out.write_text("".join(f"{cik}\n" for cik in ciks), encoding="utf-8")
ranked = sorted(ciks, key=lambda cik: hashlib.sha256(f"{seed}|{cik}".encode()).hexdigest())
day = ranked[:6]
day_one.write_text("".join(f"{cik}\n" for cik in day), encoding="utf-8")
receipt_out.write_text(
    json.dumps(
        {
            "version": "canon.entity.session.bdc.selection.v1",
            "selection_seed": seed,
            "available_ciks": len(ciks),
            "declared_day_one_ciks": day,
            "selection_method": "sha256(seed|cik) ascending, first six",
            "seeded_runtime_order": ranked,
            "sample_size": len(day),
        },
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY
printf 'selection_receipt=%s\n' "$(cat "$work_dir/selection.json")"

rules="$work_dir/rules.yaml"
cat >"$rules" <<'EOF'
version: canon.entity.session.rules.v1
keys:
  authoritative: [cik]
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
EOF

registry="$work_dir/staging-registry"
day_one_session="$work_dir/day-one-session"
day_one_obs="$work_dir/day-one.jsonl"
python3 "$converter" \
  --mode filers \
  --gold-dir "$gold_dir" \
  --ciks-file "$work_dir/day-one-ciks.txt" \
  --pick earliest \
  --out "$day_one_obs" \
  --self-check \
  --canon-bin "$canon_bin"

run_expected_json "day-one init" "$canon_bin" entity session init \
  --goal "BDC skeleton proof day one" \
  --registry "$registry" \
  --work-dir "$day_one_session" \
  --registry-id bdc-skeleton \
  --prefix STG \
  --zero-pad 6 \
  --emit json
run_expected_json "day-one ingest" "$canon_bin" entity session ingest "$day_one_session" "$day_one_obs" --emit json
day_one_ingest="$RUN_JSON"
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert not v["rejected"], v["rejected"]' "$day_one_ingest"
run_expected_json "day-one solve" "$canon_bin" entity session solve "$day_one_session" --rules "$rules" --emit json
day_one_solve="$RUN_JSON"
run_expected_json "day-one commit" "$canon_bin" entity session commit "$day_one_session" --next-version 0.1.0 --emit json
day_one_commit="$RUN_JSON"
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["outcome"] == "committed"; assert len(v["minted"]) == 6' "$day_one_commit"
run_expected_json "day-one lint" "$canon_bin" registry lint "$registry" --emit json

python3 - "$gold_dir" "$work_dir/day-one-ciks.txt" "$work_dir/day-one-lookup.jsonl" <<'PY'
import json
import pathlib
import sys

gold_dir = pathlib.Path(sys.argv[1])
selected = {
    line.strip()
    for line in pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").splitlines()
    if line.strip()
}
rows = []
for path in sorted(gold_dir.glob("*.goldset.v*.json")):
    source = json.loads(path.read_text(encoding="utf-8"))["source"]
    if source["cik"] in selected:
        rows.append({"company": source["company"], "cik": source["cik"]})
seen = {(row["company"], row["cik"]) for row in rows}
with pathlib.Path(sys.argv[3]).open("w", encoding="utf-8") as output:
    for row in sorted(seen):
        output.write(json.dumps({"company": row[0], "cik": row[1]}, sort_keys=True) + "\n")
PY
day_one_lookup="$work_dir/day-one-lookup.jsonl"
run_expected_json "day-one name lookup" "$canon_bin" "$day_one_lookup" --registry "$registry" --column company --explicit --emit json
run_expected_json "day-one CIK lookup" "$canon_bin" "$day_one_lookup" --registry "$registry" --column cik --explicit --emit json

cp -R "$registry" "$work_dir/registry-before-day-two"
day_two_session="$work_dir/day-two-session"
day_two_later="$work_dir/day-two-later.jsonl"
day_two_other="$work_dir/day-two-other.jsonl"
day_two_obs="$work_dir/day-two.jsonl"
python3 "$converter" --mode filers --gold-dir "$gold_dir" --ciks-file "$work_dir/day-one-ciks.txt" --pick later --out "$day_two_later"
python3 - "$work_dir/all-ciks.txt" "$work_dir/day-one-ciks.txt" "$work_dir/other-ciks.txt" <<'PY'
import pathlib
import sys

all_ciks = {line.strip() for line in pathlib.Path(sys.argv[1]).read_text().splitlines() if line.strip()}
day_one = {line.strip() for line in pathlib.Path(sys.argv[2]).read_text().splitlines() if line.strip()}
pathlib.Path(sys.argv[3]).write_text("".join(f"{cik}\n" for cik in sorted(all_ciks - day_one)), encoding="utf-8")
PY
python3 "$converter" --mode filers --gold-dir "$gold_dir" --ciks-file "$work_dir/other-ciks.txt" --pick all --out "$day_two_other"
cat "$day_two_later" "$day_two_other" >"$day_two_obs"

run_expected_json "day-two init" "$canon_bin" entity session init \
  --goal "BDC skeleton proof day two" \
  --registry "$registry" \
  --work-dir "$day_two_session" \
  --entity-kind organisation \
  --emit json
run_expected_json "day-two ingest" "$canon_bin" entity session ingest "$day_two_session" "$day_two_obs" --emit json
run_expected_json "day-two solve" "$canon_bin" entity session solve "$day_two_session" --rules "$rules" --emit json
run_expected_json "day-two commit" "$canon_bin" entity session commit "$day_two_session" --next-version 0.2.0 --emit json
day_two_commit="$RUN_JSON"
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["outcome"] == "committed"; assert len(v["minted"]) == 48; assert all(not row["aliases"] for row in v["attached"]); assert v["registry"]["entry_count_after"] == 108' "$day_two_commit"
run_expected_json "day-two lint" "$canon_bin" registry lint "$registry" --emit json
run_expected_json "day-two diff" "$canon_bin" registry diff --old "$work_dir/registry-before-day-two" --new "$registry" --emit json
day_two_diff="$RUN_JSON"
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["summary"]["removed"] == 0; assert v["summary"]["changed"] == 0' "$day_two_diff"

python3 - "$gold_dir" "$work_dir/all-ciks.txt" "$work_dir/all-lookup.jsonl" <<'PY'
import json
import pathlib
import sys

gold_dir = pathlib.Path(sys.argv[1])
ciks = {line.strip() for line in pathlib.Path(sys.argv[2]).read_text().splitlines() if line.strip()}
by_cik = {}
for path in sorted(gold_dir.glob("*.goldset.v*.json")):
    source = json.loads(path.read_text(encoding="utf-8"))["source"]
    if source["cik"] in ciks:
        by_cik[source["cik"]] = source["company"]
with pathlib.Path(sys.argv[3]).open("w", encoding="utf-8") as output:
    for cik in sorted(by_cik):
        output.write(json.dumps({"company": by_cik[cik], "cik": cik}, sort_keys=True) + "\n")
PY
all_lookup="$work_dir/all-lookup.jsonl"
run_expected_json "day-two name lookup" "$canon_bin" "$all_lookup" --registry "$registry" --column company --explicit --emit json
day_two_name_lookup="$RUN_JSON"
printf '%s\n' "$day_two_name_lookup" >"$work_dir/day-two-name-lookup.json"
run_expected_json "day-two CIK lookup" "$canon_bin" "$all_lookup" --registry "$registry" --column cik --explicit --emit json
day_two_cik_lookup="$RUN_JSON"
printf '%s\n' "$day_two_cik_lookup" >"$work_dir/day-two-cik-lookup.json"

# Print the complete CIK/name table for independent review. The countermetric
# is derived from the runtime lookup outputs, not from hand-labelled IDs.
python3 - "$work_dir/all-lookup.jsonl" "$work_dir/day-two-name-lookup.json" "$work_dir/day-two-cik-lookup.json" <<'PY'
import json
import pathlib
import sys

lookup_path, names_path, ciks_path = map(pathlib.Path, sys.argv[1:])
source_rows = [json.loads(line) for line in lookup_path.read_text(encoding="utf-8").splitlines() if line.strip()]
names = json.loads(names_path.read_text(encoding="utf-8"))
ciks = json.loads(ciks_path.read_text(encoding="utf-8"))

def explicit_value(value):
    # Canon renders explicit byte identifiers with a stable u8: prefix.
    return value.removeprefix("u8:")

name_to_id = {
    explicit_value(row["input"]): explicit_value(row["canonical_id"])
    for row in names["mappings"]
}
cik_to_id = {
    explicit_value(row["input"]): explicit_value(row["canonical_id"])
    for row in ciks["mappings"]
}
rows = []
for source in source_rows:
    name = source["company"]
    cik = source["cik"]
    rows.append({"canonical_id": cik_to_id[cik], "cik": cik, "name": name})
wrong_name_for_cik = sum(1 for row in rows if name_to_id[row["name"]] != row["canonical_id"])
summary = {
    "id_count": len({row["canonical_id"] for row in rows}),
    "row_count": len(rows),
    "wrong_name_for_cik": wrong_name_for_cik,
}
print(json.dumps(summary, sort_keys=True))
for row in rows:
    print(f'{row["canonical_id"]}\t{row["cik"]}\t{row["name"]}')
if summary["id_count"] != 54 or summary["wrong_name_for_cik"] != 0:
    raise SystemExit(f"countermetric failed: {json.dumps(summary, sort_keys=True)}")
PY

# Planted negatives run against copies of the staging registry. They are
# deliberately outside the positive registry so no failed commit can mutate it.
cp -R "$registry" "$work_dir/negative-key-conflict-registry"
negative_key_registry="$work_dir/negative-key-conflict-registry"
negative_key_session="$work_dir/negative-key-conflict-session"
negative_key_obs="$work_dir/negative-key-conflict.jsonl"
python3 - "$gold_dir" "$work_dir/day-one-ciks.txt" "$negative_key_obs" <<'PY'
import json
import pathlib
import sys

gold_dir = pathlib.Path(sys.argv[1])
selected = {line.strip() for line in pathlib.Path(sys.argv[2]).read_text().splitlines() if line.strip()}
row = None
for path in sorted(gold_dir.glob("*.goldset.v*.json")):
    source = json.loads(path.read_text(encoding="utf-8"))["source"]
    if source["cik"] in selected:
        row = {"obs_id": "planted-runtime-selected-wrong-key", "entity_type": "organisation", "surface": source["company"], "keys": {"cik": "9999999"}, "source": {"channel": "file", "file": "planted-key-conflict.jsonl", "lineage": "planted:key-conflict"}}
        break
if row is None:
    raise SystemExit("day-one sample has no runtime-selected incumbent")
pathlib.Path(sys.argv[3]).write_text(json.dumps(row) + "\n", encoding="utf-8")
PY
run_expected_json "negative key-conflict init" "$canon_bin" entity session init \
  --goal "BDC planted key conflict" --registry "$negative_key_registry" --work-dir "$negative_key_session" --entity-kind organisation --emit json
run_expected_json "negative key-conflict ingest" "$canon_bin" entity session ingest "$negative_key_session" "$negative_key_obs" --emit json
run_expected_json "negative key-conflict solve" "$canon_bin" entity session solve "$negative_key_session" --rules "$rules" --emit json
run_expected_failure_json "negative key-conflict commit" "$canon_bin" entity session commit "$negative_key_session" --next-version 0.3.0 --emit json
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["outcome"] == "nothing_to_commit"; assert any(row["reason"] == "key_conflict" for row in v["skipped"])' "$RUN_JSON"

cp -R "$registry" "$work_dir/negative-standalone-registry"
negative_standalone_registry="$work_dir/negative-standalone-registry"
negative_standalone_session="$work_dir/negative-standalone-session"
negative_standalone_obs="$work_dir/negative-standalone.jsonl"
cat >"$negative_standalone_obs" <<'EOF'
{"obs_id":"planted-unseen-standalone","entity_type":"organisation","surface":"Unseen Invented BDC","keys":{"cik":null},"source":{"channel":"file","file":"planted-standalone.jsonl","lineage":"planted:standalone"}}
EOF
run_expected_json "negative standalone init" "$canon_bin" entity session init \
  --goal "BDC planted standalone" --registry "$negative_standalone_registry" --work-dir "$negative_standalone_session" --entity-kind organisation --emit json
run_expected_json "negative standalone ingest" "$canon_bin" entity session ingest "$negative_standalone_session" "$negative_standalone_obs" --emit json
run_expected_json "negative standalone solve" "$canon_bin" entity session solve "$negative_standalone_session" --rules "$rules" --emit json
run_expected_failure_json "negative standalone commit" "$canon_bin" entity session commit "$negative_standalone_session" --next-version 0.3.0 --emit json
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["outcome"] == "nothing_to_commit"; assert any(row["reason"] == "standalone" for row in v["skipped"])' "$RUN_JSON"
printf '{"name":"Unseen Invented BDC"}\n' >"$work_dir/negative-standalone-lookup.jsonl"
run_expected_failure_json "negative standalone lookup" "$canon_bin" "$work_dir/negative-standalone-lookup.jsonl" --registry "$negative_standalone_registry" --column name --explicit --emit json
python3 -c 'import json,sys; v=json.loads(sys.argv[1]); assert v["summary"]["unresolved"] == 1' "$RUN_JSON"

if [[ -n "$second_source" ]]; then
  second_session="$work_dir/second-source-session"
  second_obs="$work_dir/second-source.jsonl"
  python3 - "$second_source" "$gold_dir" "$second_obs" <<'PY'
import json
import pathlib
import sys

source_path, gold_dir, output_path = map(pathlib.Path, sys.argv[1:])
gold_names = {}
for path in gold_dir.glob("*.goldset.v*.json"):
    source = json.loads(path.read_text(encoding="utf-8"))["source"]
    gold_names.setdefault(source["cik"], source["company"])
observations = []
already_resolved = 0
unknown_cik = 0
for index, line in enumerate(source_path.read_text(encoding="utf-8").splitlines(), 1):
    if not line.strip():
        continue
    row = json.loads(line)
    cik = str(row["cik"])
    name = str(row["name"])
    if cik not in gold_names:
        unknown_cik += 1
        continue
    if gold_names.get(cik) == name:
        already_resolved += 1
        continue
    observations.append({"obs_id": f"edgar-registrant:{index}", "entity_type": "organisation", "surface": name, "keys": {"cik": cik}, "source": {"channel": "file", "file": source_path.name, "lineage": "edgar:registrant"}})
output_path.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in observations), encoding="utf-8")
print(json.dumps({"already_resolved": already_resolved, "candidate_rows": len(observations), "unknown_cik": unknown_cik}, sort_keys=True))
PY
  run_expected_json "second source init" "$canon_bin" entity session init \
    --goal "BDC second source aliases" --registry "$registry" --work-dir "$second_session" --entity-kind organisation --emit json
  run_expected_json "second source ingest" "$canon_bin" entity session ingest "$second_session" "$second_obs" --emit json
  run_expected_json "second source solve" "$canon_bin" entity session solve "$second_session" --rules "$rules" --emit json
  run_expected_json "second source commit" "$canon_bin" entity session commit "$second_session" --bump patch --emit json
  printf 'second_source_counts_are_in_the_converter_report_above; inspect attached/skipped in the commit JSON\n'
else
  printf 'second_source=blocked (operator-supplied EDGAR registrant JSONL not provided)\n'
fi

python3 - "$registry" <<'PY'
import json
import pathlib
import sys

registry = pathlib.Path(sys.argv[1])
aliases = json.loads((registry / "aliases.json").read_text(encoding="utf-8"))
ids = sorted({row["canonical_id"] for row in aliases})
print(json.dumps({"registry_ids": len(ids), "alias_entries": len(aliases), "ids": ids}, sort_keys=True))
if len(ids) != 54:
    raise SystemExit(f"expected 54 registry ids after day two, found {len(ids)}")
PY

printf '\nproof_complete=true\n'
printf 'positive_countermetric_check=registry ids and CIK/name lookup outputs are printed above; no precision claim\n'
