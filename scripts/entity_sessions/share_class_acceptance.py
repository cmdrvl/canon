#!/usr/bin/env python3
"""Measure the committed share-class and footnote normalization on BDC gold sets.

This driver invokes only public Canon commands.  It retains the explicit work
directory and all artifacts for independent review; it never removes files.
The seeded examples are selected from the runtime-produced prepared surfaces,
not from a hand-labelled name list.
"""

from __future__ import annotations

import argparse
import json
import random
import subprocess
import sys
from pathlib import Path
from typing import Any


OPERATORS = "strip_footnote_marks, strip_security_tail, preserve_legal_form_with_bare"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gold-dir", type=Path, required=True)
    parser.add_argument("--canon-bin", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=20261117)
    parser.add_argument("--sample-size", type=int, default=50)
    return parser.parse_args()


def fail(message: str) -> None:
    raise SystemExit(f"share_class_acceptance.py: {message}")


def run(command: list[str], cwd: Path) -> dict[str, Any]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    (cwd / "last-command.txt").write_text(" ".join(command) + "\n", encoding="utf-8")
    (cwd / "last-stdout.txt").write_text(completed.stdout, encoding="utf-8")
    (cwd / "last-stderr.txt").write_text(completed.stderr, encoding="utf-8")
    if completed.returncode != 0:
        fail(
            f"command failed with exit {completed.returncode}: {' '.join(command)}\n"
            f"stdout={completed.stdout[-2000:]}\nstderr={completed.stderr[-2000:]}"
        )
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError:
        return {"stdout": completed.stdout}


def write_profile(path: Path) -> None:
    path.write_text(
        f"""profile: share_class_acceptance
version: 0.1.0
entity_type: organisation
identity_semantics: same_normalized_name
canonical_type: org
required_fields: [source_row_id, surface]
normalized_views:
  core:
    operators: [{OPERATORS}]
  row_key:
    operators: [ascii_trim_upper]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
  anchor_fields:
    cik: cik
    lei: lei
  surface_key_fields: [row_key]
  nullable_fields: [cik, lei]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
    - op: exact_identity
      view: core
evidence:
  support:
    - op: exact_view
      view: core
  cannot_link:
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
  aliases: share_class_acceptance.aliases
  distinct: share_class_acceptance.distinct
  relations: share_class_acceptance.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
""",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    if args.sample_size < 0:
        fail("--sample-size must be non-negative")
    gold_dir = args.gold_dir.resolve()
    canon_bin = args.canon_bin.resolve()
    work_dir = args.work_dir.resolve()
    if not gold_dir.is_dir():
        fail(f"gold-set directory does not exist: {gold_dir}")
    if not canon_bin.is_file():
        fail(f"Canon binary does not exist: {canon_bin}")
    work_dir.mkdir(parents=True, exist_ok=True)

    rows = work_dir / "borrowers.jsonl"
    converter = Path(__file__).with_name("bdc_goldsets_to_session.py").resolve()
    run(
        [
            sys.executable,
            str(converter),
            "--mode",
            "borrowers",
            "--gold-dir",
            str(gold_dir),
            "--pick",
            "all",
            "--out",
            str(rows),
        ],
        work_dir,
    )
    registry = work_dir / "registry"
    session = work_dir / "session"
    profile = work_dir / "profile.yaml"
    strategy = work_dir / "strategy.yaml"
    write_profile(profile)
    strategy.write_text(
        "block:\n  preflight: false\n  max_candidates_per_run: 50000000\n",
        encoding="utf-8",
    )
    run(
        [
            str(canon_bin),
            "registry",
            "init",
            "--registry",
            str(registry),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "5",
        ],
        work_dir,
    )
    run(
        [
            str(canon_bin),
            "entity",
            "session",
            "init",
            "--goal",
            "share class and footnote acceptance",
            "--registry",
            str(registry),
            "--work-dir",
            str(session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "5",
        ],
        work_dir,
    )
    run([str(canon_bin), "entity", "session", "ingest", str(session), str(rows)], work_dir)
    derive = run(
        [
            str(canon_bin),
            "entity",
            "session",
            "derive",
            str(session),
            "--profile",
            str(profile),
            "--strategy",
            str(strategy),
        ],
        work_dir,
    )

    surfaces_path = session / "derive-work" / "stages" / "prepare" / "surfaces.jsonl"
    surfaces = [
        json.loads(line)
        for line in surfaces_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    stripped: list[dict[str, Any]] = []
    footnote_count = 0
    security_tail_count = 0
    for surface in surfaces:
        view = surface.get("normalized_views", {}).get("core", {})
        reasons = [str(reason) for reason in view.get("reason_codes", [])]
        has_footnote = "footnote_marks" in reasons
        security_steps = [reason for reason in reasons if reason.startswith("security_tail:")]
        if has_footnote:
            footnote_count += 1
        if security_steps:
            security_tail_count += 1
        if has_footnote or security_steps:
            stripped.append(
                {
                    "surface_id": surface.get("surface_id"),
                    "primary_surface": surface.get("primary_surface"),
                    "raw_variants": surface.get("raw_variants", []),
                    "normalized": view.get("value"),
                    "steps": reasons,
                    "removed_tail": view.get("removed_tail"),
                }
            )

    if len(stripped) < args.sample_size:
        fail(
            f"runtime produced only {len(stripped)} stripped surfaces; "
            f"cannot provide the requested seeded sample of {args.sample_size}"
        )
    rng = random.Random(args.seed)
    sample = list(stripped)
    rng.shuffle(sample)
    sample = sample[: args.sample_size]
    summary = {
        "rows": sum(1 for _ in rows.open(encoding="utf-8")),
        "prepared_surfaces": len(surfaces),
        "footnote_marks_surfaces": footnote_count,
        "security_tail_surfaces": security_tail_count,
        "stripped_surfaces": len(stripped),
        "seed": args.seed,
        "sample_size": len(sample),
        "operators": ["strip_footnote_marks", "strip_security_tail", "preserve_legal_form_with_bare"],
        "derive_records_by_type": derive.get("records_by_type", derive),
        "work_dir": str(work_dir),
    }
    (work_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (work_dir / "stripped-examples.json").write_text(
        json.dumps(sample, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(json.dumps(summary, sort_keys=True))
    for index, example in enumerate(sample, start=1):
        print(
            f"{index:03d}: {example['primary_surface']} -> {example['normalized']} "
            f"steps={','.join(example['steps'])} removed_tail={example['removed_tail']!r}"
        )


if __name__ == "__main__":
    main()
