# Changelog

## Unreleased (0.17.0)

canon is now presented as three things: exact lookup, versioned registries and
evidence sessions. The released binary still contains every command group.

Breaking:

- `canon entity alias-withholding` and `canon entity generalization` are
  removed, with their `canon.evaluation.*` contracts and the
  `canon.evaluation.corpus.v1` schema file. There is no replacement command.
  `canon entity candidate-recall`, `calibrate sweep` and `calibrate em` stay.
- The session engine no longer knows the CMBS relation `servicer_of`, which
  shipped in 0.16.0. Role vocabulary is to come from a profile or session rules.
- `operator.json` and `canon --describe` are now the `operator.v1` document
  (`version` became `tool_version`, `subcommands` became `commands`), and
  `canon doctor --json` emits `cmdrvl.doctor.v1`. The old per-tool doctor schema
  ids are retired.
- For source builds only: a plain `cargo build` is the core. `canon geo` needs
  the `geo` feature, and `canon entity link` with its `src/resolve/` engine needs
  the `workbench` feature. `geo` turns `workbench` on. Released binaries are
  built with `--all-features` and keep both. A build without `workbench` does not
  list `entity link`, and refuses (`E_ENTITY_ARTIFACT_CONTRACT`) to export a
  review from a link artifact or to promote a result that is bound to one.

Evidence sessions:

- Several workers can share one session. `entity session frontier --shard
  <INDEX/TOTAL>` splits the open work, `frontier claim --worker <ID> --limit <N>
  --ttl-secs <S>` leases a batch to one worker and `frontier release` returns
  it. A lease survives a re-solve.
- `ingest` and `assert` run by concurrent processes retry on a stale head, up to
  a bound, instead of failing or losing rows.
- Places resolve through the same loop as names. Grains (site, parcel, building,
  unit, address) are registry kinds. New evidence types: `same_parcel_id`,
  `same_apn`, `address_canonical_match`, `point_in_parcel`, `footprint_overlap`,
  `deed_chain_link`, `assessor_corroboration`, and the vetoes
  `conflicting_authoritative_apn`, `disjoint_parcels` and `grain_mismatch`.
- `ingest` and `assert` read `.parquet` files.
- `session derive` emits `same_slot` evidence from slot fields a profile
  declares.
- A rules file may add `strip_legal_suffix` to
  `incumbent_attach.name_equivalent_steps`, so a later spelling that differs from
  a committed alias only by legal form attaches when that is unambiguous.
- Person names drop trailing credentials ("Dr. Jane Doe, MD"), and `derive`
  compares new spellings of a person with committed ones.
- A relation keeps its identity when the session is solved again.

Lookup and registries:

- Lookup reads `.parquet` and `.xlsx` input through the shared `cmdrvl-tabular`
  reader, with `--sheet <name|index>` for workbooks. `--emit csv` still needs CSV
  input. Other commands read the same formats as before.
- `--json` is shorthand for `--emit json` on lookup and on every registry
  command.
- `registry export --format dbt-seed` and `key-crosswalk` take `--parquet`. The
  search index lists each authoritative session key under its own namespace, and
  entity cards and the search index show cross-grain relations.
- `canon q.csv --registry registry --column name` works: an option value that
  happens to name a subcommand is read as a value.
- `registry mint-batch --prefix` no longer refuses a prefix that differs from
  the registry default. Lint's `scope_collision` checks scoped entries only.
- New discovery commands: `canon doctor triage`, `canon capabilities --json` and
  `canon robot-docs guide`.

Speed:

- The registry SQLite index is built in one transaction.
- Publishing session and entity artifacts forces data to disk far less often:
  once per new object file, and once for the object directory before the commit
  marker, instead of about seven times per object. On macOS this made
  `entity session commit` about three times faster at a few hundred objects.
- The block stage no longer scans every surface per bucket when a profile has no
  active `anchor_conflict` rule.

Documentation and development:

- The README is rewritten around lookup, registries and evidence sessions. The
  full command list moved to `docs/CLI_REFERENCE.md`, and the longer sections to
  `docs/LOOKUP_AND_REGISTRIES.md`, `docs/SCRIPTING_EXAMPLES.md`,
  `docs/ENTITY_PIPELINE.md` and `docs/GEO.md`.
- Plain `cargo test` runs unit tests and doctests only. Tests under `tests/`
  need the `integration` feature.
- The Geo executor version hashed into a Geo plan is now the fixed string
  `0.16.0` instead of the crate version, so a release no longer invalidates
  retained Geo replays.

## 0.16.0 — 2026-11-19

- Entity evidence sessions (`canon entity session ...`): `init`, `ingest`, `assert`,
  `solve`, `commit`, `status`, `explain`, `frontier`, `next`, `export`, `derive`, and
  `sample`, with typed relations, verified evidence artifacts and receipts, per-kind
  session registries (`--registries`), and a commit receipt that hands off the base,
  new and changed files, closures, and export command. A commit with nothing to write
  reports `nothing_to_commit`.
- Registry maintenance: `registry correct` (merge, detach, split, remove_cannot_link,
  and supersede, which records succession rather than sameness), `registry identity`,
  `registry check-id` (`E_BAD_ID`), `--expect-base` pinning, `registry scope-policy`
  with a role scope dimension and scope-collision lint, key aliases in their own
  files with a `key-crosswalk` export, `link-evidence` export, and bounded
  `entity-card` export.
- `standalone.split_shared_names` mints one entity per key for a shared name.
- Person and trust names (Last, First; suffixes; initials), dba/fka alternates and
  member lists, legal-form equivalence, and table-driven security-tail normalization.
  Attribute agreement (`numeric_within_tolerance`, `date_exact`, `categorical_match`)
  now counts as positive evidence.
- Retained Geo replay goldens refreshed for the new crate version
  (`GEO_EXECUTOR_VERSION` is hashed into the plan; see bd-8czc).

- Strategies can opt into `solve: {hard_cannot_link: split_direct}`. Support edges
  between directly hard-cannot-linked surfaces are cut and components recomputed,
  so numbered siblings ("Sub III" / "Sub IV") become separate entities instead of
  one `contradiction`; spelling variants stay merged and a transitive conflict
  still reports as a contradiction. Default behavior is unchanged.
- Strategies can opt into `solve: {standalone_surfaces: include}`: surfaces with no
  support edge become one-surface `escrow` components (reason
  `standalone_surface_no_support`) instead of being absent from the solve. They
  are never auto-promotable.
- New `canon registry mint-batch --from FILE.jsonl`: mint N canonical IDs, each with
  exact starting aliases, in one atomic registry version (one version bump, one
  lint pass; any refusal leaves the registry unchanged).
- The dev profile no longer uses incremental compilation (`.cargo/config.toml`), so
  `target/` stays near 5.5 GB instead of growing ~5 GB per rebuild.

## 0.15.0 — 2026-11-02

- `entity run` now executes `protected_token_conflict`. It was accepted by profile
  validation but never evaluated, so numbered siblings ("Sub III", "Sub IV",
  "Fund II") could share a component. Digit-bearing and series-Roman tokens
  (i–xxxix, two letters or longer) are protected by default; `protect` and
  `protected_tokens` params adjust that. A pair is cut only when its protected
  sets differ and the names share an unprotected token, so "Acme Coffee" vs
  "Acme Coffee Shop" is unaffected. A component containing such a cut is state
  `contradiction`. Profiles that declare the operator (including the shipped
  `cmbs_tenant_label`) may now produce hard cannot-link evidence they did not before.
- `entity run --emit summary` warns on stderr about profile operators that are
  declared but not executed by `run` (for example the shipped `regab_firm_identity`
  guards). JSON mode stays silent. Profile refusals now list the supported
  operators per lane. The operator catalog and an agent how-to are in
  `docs/ENTITY_WORKBENCH_AGENT_GUIDE.md`.
- `entity review export` on a `canon_entity_run.v1` artifact now returns the run's
  items (it returned an empty queue), read from the solve the run hash-binds; the
  queue stays bound to the run. A mismatched solve is refused.
- `entity review export` of a solve no longer emits each escrow component twice
  with the same `review_id`; `review import` refused those queues. An export with
  zero items for a source that counts review groups is refused. The
  `--artifact native-review` refusal for v1 run/solve artifacts names the
  working command.
- `registry mint`, `add-entry`, and `default-id-scheme` now set `updated` to the
  mutation date, as `registry build` does. Refusals leave `registry.json` untouched.

## 0.14.0 — 2026-09-28

- Cluster strategies can declare audit-gated policy acceptance for new identities.
  Qualification requires connected policy-supported evidence, preserves hard
  cannot-links and human overrides, records decision authority, and refuses run
  ceiling overruns. Human review remains the default.
- Policy audits execute the declared strategy against pinned inputs and complete
  identity labels, measuring pair and component precision. Promotion verifies
  the audit, strategy, manifest, and decision ledger before writing exact aliases.
- Attribute-conflict comparators support declared unit ambiguity, unknown zero
  rates, and sentinel dates. Ambiguous comparisons remain visible as soft
  negative evidence; they never create identity support. Genuine mismatches
  remain hard conflicts.
- The instrument profile excludes CUSIP `999999999` and opts into these comparator
  rules. Its content hash changes; downstream profile pins must be refreshed.

## 0.13.0 — 2026-09-28

- Entity profiles can declare exact-view, anchor, composite-key, n-gram, rare-token,
  and supplied alias-pair retrieval. Similarity operators can partition before
  selection and n-grams can use an explicit document-frequency limit. Strategies
  can override declared top-k limits; diagnostics retain the effective configuration.
- Retrieval keys produce candidates for evidence evaluation. They do not establish
  equality. `exact_identity` explicitly preserves the legacy equality-bucket contract.
- Built-in field mappings, canonical views, normalization, and blocking are data.
  Engine behavior no longer selects normalization or core views by profile name.
  Existing custom native profiles that relied on a built-in name for their prepare
  mapping must declare that mapping; `entity profile init` emits complete examples.
- Candidate generation stops at the first actual budget crossing and reports the
  operator and lower-bound counts. `entity run` writes an advisory sampled preflight
  unless the strategy sets `block.preflight: false`.
- N-gram retrieval reuses dense score storage and materializes only selected
  candidates, preserving saturated-score and tie ordering. `canon_entity_topk.v1`
  retains exact drop counts; its `dropped` list is empty rather than unbounded.
- Solver constraint evaluation groups edges once by component, preserving graph
  order and conflict decisions while avoiding a full edge scan per component.
- The instrument profile declares identifier/anchor retrieval, an issuer/maturity/rate
  composite key, and title retrieval partitioned by maturity and rate. Missing issuer
  LEIs do not silently fall back to issuer names. Large exact-key groups still consume
  pair budgets; compressed scored retrieval buckets are not part of this change.
- Fixed-decimal evidence comparisons accept leading-dot values such as `.625`
  without floating-point conversion. Malformed values and out-of-tolerance conflicts
  remain rejected or reported under the existing contracts.

Exact registry lookup and review-gated promotion remain unchanged. Profile bytes,
operator IDs, canonical-view provenance markers, and dependent artifact hashes change;
old and new workbench artifacts are not claimed byte-identical across that migration.
