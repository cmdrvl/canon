# sec10d Reg AB Contract Fixture Provenance

These fixtures are canon-owned, redacted contract snapshots for the `regab_firm_identity`
entity profile. They freeze the local shape that downstream `sec10d` migration work
must satisfy without requiring a live `sec10d` checkout.

Source context:

- Derived from the public Reg AB baseline documented in `docs/SEC10D_REGAB_BENCHMARKS.md`.
- Full source artifact reference: `sec10d_regab_org_canon_baseline_20260623T204557Z.zip`.
- Baseline SHA-256: `5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b`.
- Canon-local fixture root: `tests/fixtures/entity/regab/org_mentions/`.

Snapshot policy:

- `org_mentions.csv` and `org_mentions.jsonl` define the accepted parser-owned input shape.
- `applied_org_enrichment.jsonl` defines Snowflake-facing append-only output fields.
- `expected_summary.json` and `expected_apply.csv` are machine-readable expected outcomes.
- Raw parser fields are immutable. Canon may append canonical fields but must not rewrite parser evidence.
