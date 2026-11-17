#!/usr/bin/env python3
"""Convert sealed BDC filer gold sets into Canon evidence-session observations.

This converter deliberately carries only facts present in each gold-set source
object. It does not infer aliases, normalize names, or manufacture evidence.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path
from typing import Any, NoReturn


REQUIRED_SOURCE_FIELDS = (
    "company",
    "ticker",
    "cik",
    "form",
    "filed_date",
    "as_of_date",
    "accession_number",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("filers", "borrowers"), required=True)
    parser.add_argument("--gold-dir", type=Path, required=True)
    parser.add_argument("--ciks-file", type=Path)
    parser.add_argument("--pick", choices=("earliest", "later", "all"), required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="ingest the generated JSONL through the public session command",
    )
    parser.add_argument(
        "--canon-bin",
        type=Path,
        help="canon executable for --self-check; defaults to CANON_BIN or PATH",
    )
    return parser.parse_args()


def fail(message: str) -> NoReturn:
    print(f"bdc_goldsets_to_session.py: {message}", file=sys.stderr)
    raise SystemExit(2)


def load_ciks(path: Path | None) -> set[str] | None:
    if path is None:
        return None
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail(f"cannot read CIK selection file {path}: {error}")
    ciks = {line.strip() for line in lines if line.strip() and not line.lstrip().startswith("#")}
    if not ciks:
        fail(f"CIK selection file {path} is empty")
    return ciks


def goldset_files(gold_dir: Path) -> list[Path]:
    if not gold_dir.is_dir():
        fail(f"gold-set directory does not exist: {gold_dir}")
    files = sorted(gold_dir.glob("*.goldset.v*.json"))
    if not files:
        fail(f"no *.goldset.v*.json files found under {gold_dir}")
    return files


def load_goldset(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot parse {path}: {error}")
    if not isinstance(value, dict) or not isinstance(value.get("source"), dict):
        fail(f"{path} must contain an object-valued source")
    source = value["source"]
    missing = [field for field in REQUIRED_SOURCE_FIELDS if not isinstance(source.get(field), str)]
    if missing:
        fail(f"{path} source is missing string fields: {', '.join(missing)}")
    return value


def source_sort_key(path: Path, source: dict[str, str]) -> tuple[str, str, str, str]:
    return (
        source["filed_date"],
        source["as_of_date"],
        source["accession_number"],
        path.name,
    )


def observation(
    path: Path,
    goldset: dict[str, Any],
    gold_dir: Path,
    duplicate_accession: bool,
) -> dict[str, Any]:
    source = goldset["source"]
    try:
        relative_file = path.relative_to(gold_dir).as_posix()
    except ValueError:
        relative_file = path.name
    accession = source["accession_number"]
    obs_id = accession
    if duplicate_accession:
        obs_id = f"{accession}:{source['as_of_date']}:{path.stem}"
    return {
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": source["company"],
        "keys": {"cik": source["cik"]},
        "context": {
            "ticker": source["ticker"],
            "form": source["form"],
            "as_of_date": source["as_of_date"],
            "filed_date": source["filed_date"],
            "accession_number": accession,
        },
        "source": {
            "channel": "file",
            "file": relative_file,
            "lineage": f"edgar:cik:{source['cik']}",
        },
    }


def selected_observations(
    gold_dir: Path, ciks: set[str] | None, pick: str
) -> list[dict[str, Any]]:
    grouped: dict[str, list[tuple[Path, dict[str, Any]]]] = {}
    for path in goldset_files(gold_dir):
        goldset = load_goldset(path)
        source = goldset["source"]
        cik = source["cik"]
        if ciks is not None and cik not in ciks:
            continue
        grouped.setdefault(cik, []).append((path, goldset))

    if ciks is not None:
        missing = sorted(ciks.difference(grouped))
        if missing:
            fail(f"selected CIKs have no gold-set file: {', '.join(missing)}")
    if not grouped:
        fail("selection contains no gold-set observations")

    selected: list[tuple[Path, dict[str, Any]]] = []
    for cik in sorted(grouped):
        rows = sorted(grouped[cik], key=lambda item: source_sort_key(item[0], item[1]["source"]))
        if pick == "earliest":
            selected.extend(rows[:1])
        elif pick == "later":
            selected.extend(rows[1:])
        else:
            selected.extend(rows)

    if not selected:
        fail("selection contains no later filings")
    accession_counts = Counter(
        value["source"]["accession_number"] for _, value in selected
    )
    return [
        observation(
            path,
            value,
            gold_dir,
            accession_counts[value["source"]["accession_number"]] > 1,
        )
        for path, value in selected
    ]


BORROWER_ATTRS = ("investment_type", "maturity_date", "spread", "reference_rate")


def borrower_observations(
    gold_dir: Path, ciks: set[str] | None, pick: str
) -> list[dict[str, Any]]:
    """One observation per gold-set row: obs_id is the row id, the surface is the
    portfolio company, attrs and context come straight from the row and its fund,
    and the lineage is the fund's CIK. Nothing is inferred, normalised or
    asserted: a null in the source stays absent here."""
    grouped: dict[str, list[tuple[Path, dict[str, Any]]]] = {}
    for path in goldset_files(gold_dir):
        goldset = load_goldset(path)
        cik = goldset["source"]["cik"]
        if ciks is not None and cik not in ciks:
            continue
        grouped.setdefault(cik, []).append((path, goldset))
    if ciks is not None:
        missing = sorted(ciks.difference(grouped))
        if missing:
            fail(f"selected CIKs have no gold-set file: {', '.join(missing)}")
    if not grouped:
        fail("selection contains no gold-set files")

    selected: list[tuple[Path, dict[str, Any]]] = []
    for cik in sorted(grouped):
        files = sorted(grouped[cik], key=lambda item: source_sort_key(item[0], item[1]["source"]))
        if pick == "earliest":
            selected.extend(files[:1])
        elif pick == "later":
            selected.extend(files[1:])
        else:
            selected.extend(files)
    if not selected:
        fail("selection contains no later filings")

    observations: list[dict[str, Any]] = []
    seen: set[str] = set()
    for path, goldset in selected:
        source = goldset["source"]
        rows = goldset.get("rows")
        if not isinstance(rows, list):
            fail(f"{path} has no rows array")
        for row in rows:
            if not isinstance(row, dict):
                continue
            row_id = row.get("row_id")
            company = row.get("portfolio_company")
            if not isinstance(row_id, str) or not isinstance(company, str) or not company.strip():
                continue
            if row_id in seen:
                fail(f"duplicate row_id {row_id} across the selected gold sets")
            seen.add(row_id)
            attrs = {
                name: row[name]
                for name in BORROWER_ATTRS
                if isinstance(row.get(name), (str, int, float)) and str(row[name]).strip()
            }
            observations.append(
                {
                    "obs_id": row_id,
                    "entity_type": "organisation",
                    "surface": company,
                    "keys": {},
                    "attrs": attrs,
                    "context": {
                        "fund": source["company"],
                        "as_of_date": source["as_of_date"],
                    },
                    "source": {
                        "channel": "file",
                        "file": path.name,
                        "lineage": f"edgar:cik:{source['cik']}",
                    },
                }
            )
    if not observations:
        fail("selection contains no borrower rows")
    return observations


def canon_binary(explicit: Path | None) -> Path:
    candidates: list[str] = []
    if explicit is not None:
        candidates.append(os.fspath(explicit))
    if os.environ.get("CANON_BIN"):
        candidates.append(os.environ["CANON_BIN"])
    candidates.extend(("target/debug/canon", "target/release/canon"))
    from_path = shutil.which("canon")
    if from_path is not None:
        candidates.append(from_path)
    for candidate in candidates:
        path = Path(candidate)
        if path.is_file() and os.access(path, os.X_OK):
            return path
    fail("--self-check needs an executable canon; set --canon-bin or CANON_BIN")


def run_self_check(output: Path, explicit_binary: Path | None) -> None:
    binary = canon_binary(explicit_binary)
    with tempfile.TemporaryDirectory(prefix="canon-bdc-self-check-") as temporary:
        root = Path(temporary)
        registry = root / "registry"
        session = root / "session"
        init = subprocess.run(
            [
                os.fspath(binary),
                "entity",
                "session",
                "init",
                "--goal",
                "BDC gold-set converter self-check",
                "--registry",
                os.fspath(registry),
                "--work-dir",
                os.fspath(session),
                "--registry-id",
                "bdc-self-check",
                "--prefix",
                "CHK",
                "--zero-pad",
                "3",
                "--emit",
                "json",
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=120,
        )
        if init.returncode != 0:
            sys.stderr.write(init.stderr)
            sys.stderr.write(init.stdout)
            fail(f"self-check session init failed with exit {init.returncode}")
        sys.stderr.write(init.stderr)
        sys.stderr.write(init.stdout)

        ingest = subprocess.run(
            [
                os.fspath(binary),
                "entity",
                "session",
                "ingest",
                os.fspath(session),
                os.fspath(output),
                "--emit",
                "json",
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=120,
        )
        sys.stderr.write(ingest.stderr)
        sys.stderr.write(ingest.stdout)
        if ingest.returncode != 0:
            fail(f"self-check ingest failed with exit {ingest.returncode}")
        try:
            payload = json.loads(ingest.stdout)
        except json.JSONDecodeError as error:
            fail(f"self-check ingest did not emit JSON: {error}")
        rejected = payload.get("rejected")
        if not isinstance(rejected, list) or rejected:
            fail(f"self-check ingest rejected rows: {json.dumps(rejected, sort_keys=True)}")


def main() -> None:
    args = parse_args()
    ciks = load_ciks(args.ciks_file)
    if args.mode == "borrowers":
        rows = borrower_observations(args.gold_dir, ciks, args.pick)
    else:
        rows = selected_observations(args.gold_dir, ciks, args.pick)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8", newline="\n") as output:
        for row in rows:
            output.write(json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
            output.write("\n")
    if args.self_check:
        run_self_check(args.out, args.canon_bin)


if __name__ == "__main__":
    main()
