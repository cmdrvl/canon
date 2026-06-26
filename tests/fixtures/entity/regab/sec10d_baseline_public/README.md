# sec10d Reg AB Baseline Public Fixture Slice

Source:
`/Users/zacharyruiz/Downloads/sec10d_regab_org_canon_baseline_20260623T204557Z.zip`

Source zip SHA-256:
`5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b`

This directory is a compact public-data fixture slice for normal CI. It is
derived from the full baseline zip but intentionally does not include the full
122MB mention CSVs or 400MB+ enriched JSONL files.

Included:

- `org_mentions_selected.csv`: one real input mention row for each of the 46
  observed firm surfaces.
- `org_mentions_selected.canon.csv`: the same rows with baseline
  `org_canon_id` replay output.
- `org_lookup_expected.map.json`: the exact 46-surface baseline lookup map.
- `org_mentions_summary.json`, `org_resolution_summary.json`, and
  `org_review_queue.csv`: small baseline summaries and the header-only review
  queue.
- `registry_snapshot/firms/`: the small firms registry snapshot used by the
  baseline, excluding the generated sqlite index ignored by the repo.
- `enriched_samples/*.selected.jsonl`: representative append-only enriched
  records for attestations, platform rosters, and servicer schedules.
- `fixture_slice.json`: machine-readable slice metadata.

The full zip remains useful for operator-tier benchmarks. Normal CI should use
this slice to prove input-shape compatibility, exact mapping parity, parser
field preservation, append-only output fields, and hierarchy anti-collapse.

Guard rows:

- `REGAB-OBS-002`: selected rows cover all 46 observed firm surfaces and all 31
  canonical ids from the full baseline.
- `REGAB-HIER-001`: `PNC Bank, National Association` remains `ORG-034`, while
  `Midland Loan Services, a division of PNC Bank, National Association` remains
  `ORG-035`; affiliation text is not same-firm evidence.
- `REGAB-HIER-002`: `Wells Fargo Bank, National Association` remains `ORG-012`,
  while `Wells Fargo Commercial Mortgage Servicing, a division of Wells Fargo
  Bank, National Association` remains `ORG-053`.
