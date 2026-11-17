#!/usr/bin/env python3
"""Measure legal-form equivalence on a BDC borrower gold-set run.

The Canon runs in this driver use only public commands.  The stand-in is an
independent, intentionally simple baseline: Unicode casefold, replace
punctuation with spaces, collapse whitespace, and remove exactly one trailing
legal-form phrase from a declared list.  It is a comparison instrument, not a
truth label and not Canon's runtime lookup policy.

The work directory is required and is never removed.  This keeps the command
re-executable and leaves every input, profile, command output, derive artifact,
comparison, and seeded Canon-only sample available for independent review.
"""

from __future__ import annotations

import argparse
import csv
import json
import random
import re
import string
import subprocess
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path
from typing import Any


# This list is deliberately maintained here rather than imported from Canon:
# the stand-in must remain an independent measuring baseline.  Longest phrases
# are tried first so "pty ltd" wins over its trailing "ltd" word.
STAND_IN_LEGAL_FORMS = (
    "limited liability company",
    "limited partnership",
    "national association",
    "proprietary limited",
    "and co",
    "pty ltd",
    "incorporated",
    "corporation",
    "company",
    "limited",
    "trust",
    "gmbh",
    "lllp",
    "llp",
    "l lc",
    "llc",
    "ltd",
    "inc",
    "corp",
    "plc",
    "co",
    "lp",
    "ulc",
    "spa",
    "sarl",
    "sa",
    "bv",
    "nv",
    "ag",
    "ab",
    "aps",
    "oy",
    "bank",
    "n a",
)
FORM_RE = re.compile(
    r"(?<![a-z])(?:incorporated|corporation|company|limited partnership|"
    r"limited liability company|proprietary limited|national association|"
    r"pty ltd|lllp|llp|llc|ltd|inc|corp|plc|co|lp|ulc|spa|sarl|sa|bv|nv|"
    r"gmbh|ag|ab|aps|oy|trust|bank|n a)(?![a-z])"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gold-dir", type=Path, required=True)
    parser.add_argument("--canon-bin", type=Path, required=True)
    parser.add_argument(
        "--work-dir",
        type=Path,
        required=True,
        help="explicit retained work directory; this script never deletes it",
    )
    parser.add_argument("--seed", type=int, default=20261117)
    parser.add_argument("--sample-size", type=int, default=100)
    parser.add_argument(
        "--stand-in-evidence",
        action="store_true",
        required=True,
        help="record the independent casefold/punctuation/one-form baseline",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    raise SystemExit(f"legal_form_acceptance.py: {message}")


def run(command: list[str], cwd: Path) -> dict[str, Any]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.stdout:
        (cwd / "last-stdout.txt").write_text(completed.stdout, encoding="utf-8")
    if completed.stderr:
        (cwd / "last-stderr.txt").write_text(completed.stderr, encoding="utf-8")
    if completed.returncode != 0:
        fail(
            "command failed with exit "
            f"{completed.returncode}: {' '.join(command)}\n"
            f"stdout={completed.stdout[-2000:]}\n"
            f"stderr={completed.stderr[-2000:]}"
        )
    try:
        payload: Any = json.loads(completed.stdout)
    except json.JSONDecodeError:
        payload = {"stdout": completed.stdout}
    return payload


def normalized_baseline(surface: str) -> str:
    folded = unicodedata.normalize("NFKC", surface).casefold()
    translated = "".join(
        " " if character in string.punctuation or unicodedata.category(character).startswith("P") else character
        for character in folded
    )
    tokens = " ".join(translated.split()).split()
    for phrase in sorted(STAND_IN_LEGAL_FORMS, key=lambda value: (len(value.split()), value), reverse=True):
        phrase_tokens = phrase.split()
        if len(tokens) > len(phrase_tokens) and tokens[-len(phrase_tokens) :] == phrase_tokens:
            tokens = tokens[: -len(phrase_tokens)]
            break
    return " ".join(tokens)


def baseline_groups(rows_path: Path) -> set[tuple[str, ...]]:
    by_key: dict[str, set[str]] = defaultdict(set)
    with rows_path.open(encoding="utf-8", newline="") as stream:
        for row in csv.DictReader(stream):
            surface = row.get("surface", "")
            key = normalized_baseline(surface)
            if key and surface:
                by_key[key].add(surface)
    return {tuple(sorted(spellings)) for spellings in by_key.values() if len(spellings) >= 2}


def canon_groups(session_dir: Path) -> tuple[set[tuple[str, ...]], int]:
    rows: dict[str, str] = {}
    with (session_dir / "derive-work" / "rows.csv").open(encoding="utf-8", newline="") as stream:
        for row in csv.DictReader(stream):
            rows[row["obs_id"]] = row["surface"]

    parent: dict[str, str] = {}

    def find(item: str) -> str:
        parent.setdefault(item, item)
        while parent[item] != item:
            parent[item] = parent[parent[item]]
            item = parent[item]
        return item

    records = 0
    with (session_dir / "derive-work" / "assertions.jsonl").open(encoding="utf-8") as stream:
        for line in stream:
            record = json.loads(line)
            if record.get("type") != "name_equivalent":
                continue
            records += 1
            left = record.get("left")
            right = record.get("right")
            if isinstance(left, str) and isinstance(right, str):
                parent[find(left)] = find(right)

    components: dict[str, set[str]] = defaultdict(set)
    for item in parent:
        components[find(item)].add(item)
    groups = set()
    for component in components.values():
        spellings = tuple(sorted({rows[item] for item in component if item in rows}))
        if len(spellings) >= 2:
            groups.add(spellings)
    return groups, records


def write_profile(path: Path, operator: str, with_bare_view: bool) -> None:
    views = "  core:\n    operators: [%s]\n" % operator
    mapping = "    core: surface\n"
    blocking = "    - op: exact_identity\n      view: core\n"
    support = "    - op: exact_view\n      view: core\n"
    if with_bare_view:
        views += "  bare:\n    operators: [preserve_legal_form_with_bare]\n"
        mapping += "    bare: surface\n"
        blocking += "    - op: exact_identity\n      view: bare\n"
        support += "    - op: exact_view\n      view: bare\n"
    path.write_text(
        f"""profile: legal_form_acceptance
version: 0.1.0
entity_type: organisation
identity_semantics: same_normalized_name
canonical_type: org
required_fields: [source_row_id, surface]
normalized_views:
{views}  row_key:
    operators: [ascii_trim_upper]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
{mapping}    row_key: source_row_id
  anchor_fields:
    cik: cik
    lei: lei
  surface_key_fields: [row_key]
  nullable_fields: [cik, lei]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
{blocking}evidence:
  support:
{support}  cannot_link:
    - op: protected_token_conflict
      view: core
      params:
        score_units: "0"
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: legal_form_acceptance.aliases
  distinct: legal_form_acceptance.distinct
  relations: legal_form_acceptance.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
""",
        encoding="utf-8",
    )


def run_profile(binary: Path, work: Path, label: str, rows: Path, operator: str, with_bare_view: bool) -> tuple[set[tuple[str, ...]], int, dict[str, Any]]:
    registry = work / f"{label}-registry"
    session = work / f"{label}-session"
    profile = work / f"{label}-profile.yaml"
    strategy = work / f"{label}-strategy.yaml"
    write_profile(profile, operator, with_bare_view)
    strategy.write_text("block:\n  preflight: false\n  max_candidates_per_run: 50000000\n", encoding="utf-8")
    run(
        [
            str(binary), "registry", "init", "--registry", str(registry),
            "--registry-id", "organisations", "--prefix", "ORG", "--zero-pad", "5",
        ],
        work,
    )
    run(
        [
            str(binary), "entity", "session", "init", "--goal", f"legal form {label}",
            "--registry", str(registry), "--work-dir", str(session),
            "--registry-id", "organisations", "--prefix", "ORG", "--zero-pad", "5",
        ],
        work,
    )
    run([str(binary), "entity", "session", "ingest", str(session), str(rows)], work)
    derive = run(
        [
            str(binary), "entity", "session", "derive", str(session),
            "--profile", str(profile), "--strategy", str(strategy),
        ],
        work,
    )
    groups, records = canon_groups(session)
    (work / f"{label}-groups.json").write_text(
        json.dumps([list(group) for group in sorted(groups)], indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return groups, records, derive


def form_tokens(spelling: str) -> set[str]:
    return {match.group(0).strip().casefold() for match in FORM_RE.finditer(normalized_baseline(spelling))}


def main() -> None:
    args = parse_args()
    if args.sample_size < 0:
        fail("--sample-size must be non-negative")
    args.gold_dir = args.gold_dir.resolve()
    args.canon_bin = args.canon_bin.resolve()
    args.work_dir = args.work_dir.resolve()
    if not args.gold_dir.is_dir():
        fail(f"gold-set directory does not exist: {args.gold_dir}")
    if not args.canon_bin.is_file():
        fail(f"Canon binary does not exist: {args.canon_bin}")
    args.work_dir.mkdir(parents=True, exist_ok=True)
    rows = args.work_dir / "borrowers.jsonl"
    converter = Path(__file__).with_name("bdc_goldsets_to_session.py").resolve()
    run(
        [
            sys.executable, str(converter), "--mode", "borrowers", "--gold-dir", str(args.gold_dir),
            "--pick", "all", "--out", str(rows),
        ],
        args.work_dir,
    )
    print(f"retained_work_dir={args.work_dir}")
    print(f"rows={sum(1 for _ in rows.open(encoding='utf-8'))}")
    print("stand_in_evidence=enabled")

    old_groups, old_records, old_derive = run_profile(args.canon_bin, args.work_dir, "old", rows, "legal_basename", False)
    preserve_groups, preserve_records, preserve_derive = run_profile(
        args.canon_bin, args.work_dir, "preserve", rows, "preserve_legal_form", False
    )
    canon_groups_value, canon_records, canon_derive = run_profile(
        args.canon_bin, args.work_dir, "bare", rows, "preserve_legal_form", True
    )
    stand_in_groups = baseline_groups(args.work_dir / "bare-session" / "derive-work" / "rows.csv")
    both = canon_groups_value & stand_in_groups
    canon_only = sorted(canon_groups_value - stand_in_groups)
    script_only = sorted(stand_in_groups - canon_groups_value)
    sample_rng = random.Random(args.seed)
    sample = sorted(canon_only)
    sample_rng.shuffle(sample)
    sample = sample[: args.sample_size]
    suspected_false_merges = sum(
        len(set().union(*(form_tokens(spelling) for spelling in group))) > 1
        for group in sample
    )
    summary = {
        "rows": sum(1 for _ in rows.open(encoding="utf-8")),
        "old_legal_basename_groups": len(old_groups),
        "old_records": old_records,
        "preserve_legal_form_groups": len(preserve_groups),
        "preserve_records": preserve_records,
        "canon_bare_equivalence_groups": len(canon_groups_value),
        "canon_records": canon_records,
        "stand_in_groups": len(stand_in_groups),
        "groups_in_both": len(both),
        "canon_only": len(canon_only),
        "script_only": len(script_only),
        "seed": args.seed,
        "canon_only_sample_size": len(sample),
        "suspected_false_merges_in_sample": suspected_false_merges,
        "suspected_false_merge_definition": "sample group contains more than one distinct legal-form token; this is not labelled precision",
        "stand_in_logic": "Unicode NFKC casefold, replace punctuation with spaces, collapse whitespace, remove exactly one longest trailing declared legal-form phrase",
    }
    (args.work_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (args.work_dir / "canon-only-sample.json").write_text(
        json.dumps([list(group) for group in sample], indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(summary, sort_keys=True))
    print("canon-only seeded sample:")
    for index, group in enumerate(sample, start=1):
        print(f"{index:03d}: {' || '.join(group)}")
    print(f"old_derive={old_derive.get('records_by_type', old_derive)}")
    print(f"preserve_derive={preserve_derive.get('records_by_type', preserve_derive)}")
    print(f"bare_derive={canon_derive.get('records_by_type', canon_derive)}")


if __name__ == "__main__":
    main()
