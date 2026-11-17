#!/usr/bin/env python3
"""Measure legal-form equivalence on a BDC borrower gold-set run.

The Canon runs in this driver use only public commands.  The stand-in is an
independent, intentionally simple baseline: Unicode casefold, replace
punctuation with spaces, collapse whitespace, and remove exactly one trailing
legal-form phrase from a declared list.  It is a comparison instrument, not a
truth label and not Canon's runtime lookup policy.

The work directory is required and is never removed.  This keeps the command
re-executable and leaves every input, profile, command output, derive artifact,
comparison, counting audit, stand-in-only reason list, and seeded Canon-only
sample/judgement available for independent review.
"""

from __future__ import annotations

import argparse
import csv
import json
import random
import string
import subprocess
import sys
import unicodedata
from collections import Counter, defaultdict
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
STAND_IN_FORM_ALIASES = (
    ("limited liability company", "llc"),
    ("limited partnership", "lp"),
    ("national association", "n_a"),
    ("proprietary limited", "pty_ltd"),
    ("and co", "and_co"),
    ("pty ltd", "pty_ltd"),
    ("incorporated", "inc"),
    ("corporation", "corp"),
    ("company", "co"),
    ("limited", "ltd"),
    ("trust", "trust"),
    ("gmbh", "gmbh"),
    ("lllp", "lllp"),
    ("llp", "llp"),
    ("l l c", "llc"),
    ("llc", "llc"),
    ("ltd", "ltd"),
    ("inc", "inc"),
    ("corp", "corp"),
    ("plc", "plc"),
    ("co", "co"),
    ("lp", "lp"),
    ("l p", "lp"),
    ("ulc", "ulc"),
    ("spa", "spa"),
    ("s p a", "spa"),
    ("sarl", "sarl"),
    ("s a r l", "sarl"),
    ("sa", "sa"),
    ("s a", "sa"),
    ("bv", "bv"),
    ("b v", "bv"),
    ("nv", "nv"),
    ("n v", "nv"),
    ("ag", "ag"),
    ("ab", "ab"),
    ("aps", "aps"),
    ("oy", "oy"),
    ("bank", "bank"),
    ("n a", "n_a"),
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


def normalized_tokens(surface: str) -> list[str]:
    folded = unicodedata.normalize("NFKC", surface).casefold()
    translated = "".join(
        " "
        if character in string.punctuation or unicodedata.category(character).startswith("P")
        else character
        for character in folded
    )
    return " ".join(translated.split()).split()


def canonical_form_signature(surface: str) -> tuple[str, ...]:
    """Return the ordered trailing legal-form sequence in a spelling.

    This is a measurement aid, not a second Canon normalizer.  It lets the
    report distinguish a genuine cross-form stand-in merge from a spelling
    variant such as ``Co``/``Company`` before the same final ``LLC``.
    """
    tokens = normalized_tokens(surface)
    aliases = sorted(STAND_IN_FORM_ALIASES, key=lambda item: (len(item[0].split()), item[0]), reverse=True)
    forms: list[str] = []
    while tokens:
        for phrase, canonical in aliases:
            phrase_tokens = phrase.split()
            if len(tokens) > len(phrase_tokens) and tokens[-len(phrase_tokens) :] == phrase_tokens:
                forms.insert(0, canonical)
                tokens = tokens[: -len(phrase_tokens)]
                break
        else:
            break
    return tuple(forms)


def baseline_metrics(rows_path: Path) -> dict[str, Any]:
    records: list[dict[str, str]] = []
    with rows_path.open(encoding="utf-8", newline="") as stream:
        for row in csv.DictReader(stream):
            surface = row.get("surface", "")
            key = normalized_baseline(surface)
            if key and surface:
                records.append({**row, "_baseline_key": key})
    by_key: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in records:
        by_key[row["_baseline_key"]].append(row)
    groups = {
        tuple(sorted({row["surface"] for row in group}))
        for group in by_key.values()
        if len({row["surface"] for row in group}) >= 2
    }

    def partition_groups(fields: tuple[str, ...]) -> int:
        partitions: dict[tuple[str, ...], dict[str, set[str]]] = defaultdict(lambda: defaultdict(set))
        for row in records:
            partition = tuple(row.get(field, "") for field in fields)
            partitions[partition][row["_baseline_key"]].add(row["surface"])
        return sum(
            len(spellings) >= 2
            for partition in partitions.values()
            for spellings in partition.values()
        )

    distinct_sizes = [len(group) for group in groups]
    record_sizes = [len(group) for group in by_key.values() if len({row["surface"] for row in group}) >= 2]
    return {
        "groups": groups,
        "distinct_spelling_pairs": sum(size * (size - 1) // 2 for size in distinct_sizes),
        "distinct_spelling_star_edges": sum(size - 1 for size in distinct_sizes),
        "duplicate_inclusive_pairs": sum(size * (size - 1) // 2 for size in record_sizes),
        "fund_period_groups": partition_groups(("context_fund", "context_as_of_date")),
        "as_of_date_groups": partition_groups(("context_as_of_date",)),
    }


def baseline_groups(rows_path: Path) -> set[tuple[str, ...]]:
    return baseline_metrics(rows_path)["groups"]


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


def prepared_bare_details(session_dir: Path) -> dict[str, set[tuple[str, tuple[str, ...]]]]:
    details: dict[str, set[tuple[str, tuple[str, ...]]]] = defaultdict(set)
    path = session_dir / "derive-work" / "stages" / "prepare" / "surfaces.jsonl"
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            record = json.loads(line)
            view = record.get("normalized_views", {}).get("bare")
            if not isinstance(view, dict):
                continue
            value = view.get("value")
            reasons = view.get("reason_codes")
            if isinstance(record.get("primary_surface"), str) and isinstance(value, str) and isinstance(reasons, list):
                details[record["primary_surface"]].add((value, tuple(str(reason) for reason in reasons)))
    return details


def stand_in_only_review(
    stand_in_groups: set[tuple[str, ...]],
    canon_groups_value: set[tuple[str, ...]],
    bare_details: dict[str, set[tuple[str, tuple[str, ...]]]],
) -> list[dict[str, Any]]:
    review: list[dict[str, Any]] = []
    for group in sorted(stand_in_groups - canon_groups_value):
        signatures = {canonical_form_signature(spelling) for spelling in group}
        values = {
            value
            for spelling in group
            for value, _reasons in bare_details.get(spelling, set())
        }
        supersets = [candidate for candidate in canon_groups_value if set(group) < set(candidate)]
        if supersets:
            reason_code = "canon_component_superset"
            reason = "Canon has a component containing this stand-in group plus another spelling; exact tuple comparison is not a Canon miss."
        elif len(signatures) > 1:
            reason_code = "stand_in_overmerge_distinct_legal_forms"
            reason = "The stand-in joins distinct trailing legal-form signatures; Canon refuses the cross-form merge by contract."
        elif len(values) > 1:
            reason_code = "canon_normalized_values_differ"
            reason = "The stand-in key is shared, but Canon's declared normalized values differ; inspect the form table before adding a generic step."
        else:
            reason_code = "canon_edge_or_group_set_mismatch"
            reason = "The normalized value agrees but no exact Canon group was emitted; this is the only category requiring code investigation."
        review.append(
            {
                "spellings": list(group),
                "reason_code": reason_code,
                "reason": reason,
                "stand_in_form_signatures": [list(signature) for signature in sorted(signatures)],
                "canon_bare_values": sorted(values),
            }
        )
    return review


def judge_canon_only_group(group: tuple[str, ...]) -> dict[str, Any]:
    signatures = {canonical_form_signature(spelling) for spelling in group}
    nonempty = [signature for signature in signatures if signature]
    intended_bare = (
        len(signatures) == 2
        and () in signatures
        and len(nonempty) == 1
        and len(nonempty[0]) == 1
        and nonempty[0][0] not in {"bank", "trust", "n_a"}
    )
    if len(signatures) <= 1 or intended_bare:
        return {
            "spellings": list(group),
            "false_merge": False,
            "reason": "The spellings share one canonical trailing-form sequence or are the declared bare-plus-one-form equivalence.",
            "form_signatures": [list(signature) for signature in sorted(signatures)],
        }
    return {
        "spellings": list(group),
        "false_merge": True,
        "reason": "The spellings carry different canonical trailing-form sequences; this is a possible form-handling false merge and requires operator review.",
        "form_signatures": [list(signature) for signature in sorted(signatures)],
    }


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
    stand_in_metrics = baseline_metrics(args.work_dir / "bare-session" / "derive-work" / "rows.csv")
    stand_in_groups = stand_in_metrics["groups"]
    both = canon_groups_value & stand_in_groups
    canon_only = sorted(canon_groups_value - stand_in_groups)
    script_only = sorted(stand_in_groups - canon_groups_value)
    bare_details = prepared_bare_details(args.work_dir / "bare-session")
    stand_in_only_rows = stand_in_only_review(stand_in_groups, canon_groups_value, bare_details)
    sample_rng = random.Random(args.seed)
    sample = sorted(canon_only)
    sample_rng.shuffle(sample)
    sample = sample[: args.sample_size]
    canon_only_judgement = [judge_canon_only_group(group) for group in sample]
    suspected_false_merges = sum(item["false_merge"] for item in canon_only_judgement)
    checked_counts = {
        len(stand_in_groups),
        stand_in_metrics["distinct_spelling_pairs"],
        stand_in_metrics["distinct_spelling_star_edges"],
        stand_in_metrics["duplicate_inclusive_pairs"],
        stand_in_metrics["fund_period_groups"],
        stand_in_metrics["as_of_date_groups"],
    }
    floor_reproduced = 1074 in checked_counts
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
        "suspected_false_merge_definition": "sample group has different canonical trailing-form signatures after excluding the declared bare-plus-one-form relation; this is not labelled precision",
        "stand_in_distinct_spelling_pairs": stand_in_metrics["distinct_spelling_pairs"],
        "stand_in_distinct_spelling_star_edges": stand_in_metrics["distinct_spelling_star_edges"],
        "stand_in_duplicate_inclusive_pairs": stand_in_metrics["duplicate_inclusive_pairs"],
        "stand_in_fund_period_groups": stand_in_metrics["fund_period_groups"],
        "stand_in_as_of_date_groups": stand_in_metrics["as_of_date_groups"],
        "stand_in_only_reason_counts": dict(Counter(item["reason_code"] for item in stand_in_only_rows)),
        "floor_1074_reproduced": floor_reproduced,
        "floor_1074_note": (
            "Checked group, distinct-spelling pair, distinct-spelling star-edge, "
            "duplicate-inclusive pair, fund-period, and as-of-date counts: "
            f"{sorted(checked_counts)}; "
            f"1,074 {'is' if floor_reproduced else 'is not'} reproduced by any checked count."
        ),
        "stand_in_logic": "Unicode NFKC casefold, replace punctuation with spaces, collapse whitespace, remove exactly one longest trailing declared legal-form phrase",
    }
    (args.work_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (args.work_dir / "canon-only-sample.json").write_text(
        json.dumps([list(group) for group in sample], indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    (args.work_dir / "stand-in-only-review.json").write_text(
        json.dumps(stand_in_only_rows, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    (args.work_dir / "canon-only-judgement.json").write_text(
        json.dumps(canon_only_judgement, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(summary, sort_keys=True))
    print("canon-only seeded sample:")
    for index, group in enumerate(sample, start=1):
        print(f"{index:03d}: {' || '.join(group)}")
    print("stand-in-only review:")
    for index, item in enumerate(stand_in_only_rows, start=1):
        print(f"{index:03d}: {item['reason_code']}: {' || '.join(item['spellings'])}")
    print("Canon-only judgement:")
    for index, item in enumerate(canon_only_judgement, start=1):
        print(f"{index:03d}: false_merge={item['false_merge']}: {item['reason']}")
    print(f"old_derive={old_derive.get('records_by_type', old_derive)}")
    print(f"preserve_derive={preserve_derive.get('records_by_type', preserve_derive)}")
    print(f"bare_derive={canon_derive.get('records_by_type', canon_derive)}")


if __name__ == "__main__":
    main()
