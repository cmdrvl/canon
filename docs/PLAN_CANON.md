# canon — Canonical Lookup and Registry Substrate

> Current core substrate specification. This document defines exact runtime
> lookup, registry files, refusal semantics, exports, packages, and shared
> invariants. Evidence workbenches under `canon entity`, including link mode,
> compile uncertain observations into reviewed registry updates before normal
> lookup runs.

## One-line promise
**Compile accepted identity knowledge into versioned registries, then replay it exactly. Know what matched, what did not, and why.**

If runtime lookup cannot resolve, say so clearly. If evidence is uncertain, keep
it in a workbench artifact, review queue/inbox, or explicit refusal; never smuggle it
into accepted runtime matches.

---

## Problem (clearly understood)
Structured data is full of identifier chaos:
- the same entity has 5 names across 3 vendors
- external IDs map to internal IDs, but the mapping version matters
- record, organization, fund, property, person, or asset labels drift across source systems
- reviewed aliases and refused ambiguities need to replay the same way next month

Today this means:
- ad-hoc VLOOKUP chains
- unmaintained Python scripts with hardcoded aliases
- "just ask Dave, he knows the mappings"
- zero reproducibility — the same input resolves differently next month

`canon` replaces that with a deterministic registry substrate and a clear
compiler boundary:

```text
messy evidence -> deterministic artifacts -> audit/review -> versioned registry -> exact replay
```

The default `canon <INPUT> --registry ... --column ...` command is the replay
step. It does not run open-ended evidence collection, provider lookup, ontology
reasoning, clustering, or probabilistic matching.

Architecture note: this plan defines the core lookup kernel and registry
substrate. Domain-specific resolution workbenches, such as `canon entity`, may
create audited registry updates, but normal `canon` lookup remains exact
registry lookup. See `docs/IDENTITY_ARCHITECTURE.md` for the boundary.

---

## Non-goals (explicit)
The core `canon` lookup path is NOT:
- a fuzzy matcher (suggestions may be probabilistic, but accepted mappings are deterministic)
- a master data management system
- a generic record linker at lookup time
- an address parser or geocoder
- a data cleansing tool
- a replacement for a proper MDM platform at scale
- an industry ontology, provider-knowledge bundle, or domain dictionary

It does not call remote providers at resolution time.
Registry materialization is an explicit maintenance workflow; normal `canon` runs still resolve input values against local versioned registries and record everything.

These non-goals do not prohibit bounded resolution workbenches. They prohibit
mixing fuzzy or multi-column matching into the production lookup kernel.
Workbench commands must emit inspectable artifacts, abstain on ambiguity, pass
audit gates before promotion, and write durable knowledge back into versioned
registries.

Industry expertise, provider semantics, domain thresholds, and adapter-specific
field mappings belong in registries, profiles, strategies, packages, or
out-of-tree extensions. They are not core defaults.

---

## CLI (v0)
```bash
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [--emit json|csv] [--canon-column <NAME>] [--map-out <PATH>] [--max-rows <N>] [--max-bytes <N>]
canon doctor health [--json]
canon doctor capabilities [--json]
canon doctor robot-docs
canon doctor --robot-triage
canon package pack --root <DIR> --package <package.json> --out <ARCHIVE>
canon package inspect <ARCHIVE> [--emit json|summary]
canon package verify <ARCHIVE> [--emit json|summary]
canon package unpack <ARCHIVE> --target <EMPTY_DIR> [--emit json|summary]
canon package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY> [--tag <TAG>] [--emit json|summary]
canon package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR> (--digest <sha256:...>|--tag <TAG>) [--emit json|summary]
canon project init <DIR> [--project-id <ID>] [--mapping-profile <REF>] [--emit json|summary]
canon project validate <DIR> [--manifest <PATH>] [--emit json|summary]
canon project describe <DIR> [--manifest <PATH>] [--emit json|summary]
canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>
canon geo materialize-home-cells --rows <ROWS.json>
canon geo tile-work --request <REQUEST.json>
canon geo reconcile-tiles --request <REQUEST.json>
canon geo solve --request <REQUEST.json>
canon geo materialize-geometry --request <REQUEST.json>
canon geo materialize-warehouse-geometry --rows <ROWS.json>
canon geo materialize-evidence --rows <ROWS.json>
canon geo materialize-h7-population --rows <ROWS.json>
canon geo compile-evidence --request <REQUEST.json>
canon geo evaluate --population <POPULATION.json>
canon inbox list --inbox <INBOX.json> [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox show --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox explain --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox stats --inbox <INBOX.json> [--policy <POLICY.json>] [--emit json|summary]
canon inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>] [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json> [--emit json|summary]
canon inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json> [--policy <POLICY.json>] [--event-key <KEY>...] [--limit <N>] [--mode cluster|link] [--emit json|summary]
canon registry build --source <SOURCE> --seed <SEED> --seed-column <COLUMN> --output <DIR> --version <VER> [--incremental] [--max-rows <N>] [--max-bytes <N>] [--batch-size <N>] [--rate-limit-ms <MS>] [--provider-config <KEY=VALUE>]
canon registry export --format dbt-seed|search-index --registry <REGISTRY> --out <PATH> [--namespace <CONTEXT>] [--source-file <FILE>...] [--canonical-type <TYPE>...] [--rule-id-prefix <PREFIX>...] [--canonical-iri-prefix <PREFIX>] [--schema-out <schema.yml>] [--anti-collapse-test-out <test.sql>] [--emit json|summary]
canon registry providers [--emit json|summary]
canon registry provider-schema <PROVIDER> [--emit json|summary]
canon registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain|json]
canon registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>] [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]
canon registry mint --registry <DIR> [--canonical-id <ID> | --prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>... [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]
canon registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict] [--bump patch|minor|major | --next-version <VER>] [--emit json|plain]
canon registry diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]
canon registry audit <SEED> --registry <REGISTRY> --column <COLUMN> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon registry lint <REGISTRY> [--profile standard|org|strategy|auto] [--emit json|summary]
canon strategy profile <INPUT> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR> [--emit json|summary]
canon strategy resolve --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy register --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --grade operator-attested|proof-attested --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--rule-id <RULE>] [--emit json|summary] [--no-witness]
canon strategy update --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--emit json|summary] [--no-witness]
canon strategy deprecate --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER> [--attested-at <RFC3339>] [--emit json|summary] [--no-witness]
canon strategy promote --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json> --next-version <VER> [--emit json|summary] [--no-witness]
canon strategy list --registry <REGISTRY> [--key-type schema|task] [--grade operator-attested|proof-attested] [--status active|deprecated] [--emit json|summary]
canon strategy explain --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]
canon entity run <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--cache-mode enabled|disabled] [--suite <DIR>] [--emit json|summary] [--no-witness]
canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--suite <DIR>] [--gold <GOLD.jsonl>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]
canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json> [--emit json|summary]
canon entity generalization --manifest <STRICT_ENVELOPE.json> [--emit json|summary]
canon entity prepare <ROWS> --profile <PROFILE> --registry <REGISTRY> --work-dir <DIR>
canon entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--emit json|summary]
canon entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity candidate-recall --manifest <MANIFEST.json> --candidates <CANDIDATES.jsonl> --diagnostics <DIAGNOSTICS.json> --exact-bucket-count <N> [--emit json|summary]
canon entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <REGISTRY> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <REGISTRY> [--work-dir <DIR>] [--emit json|summary]
canon entity audit <RESULT.json> --suite <DIR> [--emit json|summary]
canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY> --next-version <VER> [--emit json|summary]
canon entity apply <RESULT.json> --rows <ROWS> --registry <REGISTRY> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution|--allow-partial-output] [--emit json|summary]
canon entity review export <RESULT.json> [--artifact queue|native-review] [--emit json|csv|html] [--include resolved|escrow|contradictions|all]
canon entity review import <REVIEW.json|csv> --registry <REGISTRY> --next-version <VER> [--audit <AUDIT.json>] [--source-review <NATIVE_REVIEW.json>] [--emit json|summary]
canon entity explain <RESULT.json> --row <ROW_ID>|--surface-id <SURFACE_ID>|--canon-id <CANON_ID>|--escrow-id <ESCROW_ID> [--emit json|summary]
canon entity profile list [--emit json|summary]
canon entity profile init <PROFILE> --output <PATH>
```

Arguments:
- `<INPUT>`: CSV or JSONL file with IDs to canonicalize. Format is detected by file extension: `.csv` and `.tsv` are parsed as CSV; `.jsonl` and `.ndjson` are parsed as JSONL. Files with unrecognized or missing extensions are REFUSAL (`E_PARSE`). Use `-` for stdin (JSONL only; CSV requires seekable input for delimiter detection).

Options:
- `--registry <PATH>`: Registry directory (versioned). Required.
- `--column <COLUMN>`: Column containing IDs to resolve (CSV column name or JSONL field name). Required. Uses the same identifier encoding as `rvl` (`u8:<...>` or `hex:<...>` for ambiguous names).
- `--emit <json|csv>`: What goes to stdout. Default: `json` (mapping artifact). `csv` emits the original file with a canonical ID column appended — making `canon` a pipeline stage, not just an artifact producer. CSV input only; `--emit csv` with JSONL input is a REFUSAL (`E_EMIT_FORMAT`).
- `--canon-column <NAME>`: Name of the appended canonical ID column. Default: `<COLUMN>__canon` (e.g., `cusip__canon`). Only meaningful with `--emit csv`; ignored otherwise.
- `--map-out <PATH>`: Write the JSON mapping artifact to this file. Only meaningful with `--emit csv` — provides the mapping sidecar for `pack` or audit. Without it, `--emit csv` produces no JSON artifact (the witness ledger still records the run). Ignored with a stderr warning in `--emit json` mode (the mapping already IS stdout).
- `--max-rows <N>`: Refuse if input exceeds N data rows (raw row count including duplicates, excluding header). This is an I/O budget, not a cardinality limit — contrast with `summary.total` which counts unique values.
- `--max-bytes <N>`: Refuse if input file exceeds N bytes. For regular files, checked via file size before reading. For stdin (`-`), bytes are counted during streaming; refusal triggers as soon as the limit is exceeded (partial output may have been buffered — in JSON mode this is safe since output is emitted at end; stdin is JSONL-only so CSV mode is not affected).
- `--explicit`: Show `input` and `canonical_id` values verbatim in `--emit json` output. By default these values are masked as `"[REDACTED]"` for zero-retention safety and the envelope reports `"redacted": true`; with `--explicit` they are shown and `"redacted": false`.
- `--plain-json-values`: Only valid with `--explicit`. Emit valid UTF-8 JSON `input` and `canonical_id` values without the `u8:` prefix and add `input_encoding` / `canonical_id_encoding` metadata. Values with ASCII control bytes or non-text bytes remain `hex:<hex-bytes>` with `"hex"` metadata. The default `--explicit` contract remains lossless `u8:`/`hex:` identifier encoding.
- Bare `canon` (no arguments): print a short agent-oriented orientation to stderr — the canonical lookup command plus pointers to `canon --help`, `canon doctor --robot-triage`, `canon --describe`, and `canon --schema` — and exit 2. Stdout stays empty so pipelines are unaffected. This replaces the bare clap "required arguments" error.
- Intent inference for legible-but-wrong invocations (deterministic, edit distance 1):
  - An unknown long flag within one edit of a known core flag (e.g. `--regisry`, `--colum`, `--explcit`) is rejected with `error: unknown flag '<flag>'` plus `did you mean '--<flag>'?` on stderr, exit 2. Unknown flags with no near match defer to the standard clap error.
  - A misspelled top-level subcommand that would otherwise be swallowed as the positional input (e.g. `canon regstry --registry ... --column ...`) refuses with `E_PARSE`, `detail.suggested_subcommand`, and `next_command: canon <subcommand> --help` — but only when the positional value does not exist as a file, so real input files are never hijacked.
- `--version`: Print `canon <semver>` and exit 0.
- `--describe`: Emit `operator.json` as JSON to stdout and exit 0. This is the spine's standard tool identity record (tool name, version, accepted inputs, output schema, refusal codes) — used by orchestrators and `pack` to introspect tools without running them.
- `--schema`: Print JSON Schema for the mapping artifact (`canon.v0` object) to stdout and exit 0. This is the schema for `--emit json` output and `--map-out` sidecar, not a description of CSV output format.
- `--no-witness`: Suppress witness ledger append.
- `canon doctor health`, bare `canon doctor`, and `canon doctor --robot-triage`: emit read-only health/triage reports and exit `0` when compiled contract parity is healthy, `1` when the report is emitted but unhealthy, and `2` on CLI/refusal errors. `canon doctor capabilities` and `canon doctor robot-docs` emit successful read-only reports with exit `0`; CLI/refusal errors still exit `2`.

### Config footprint and witness ledger

`canon` is self-contained under the CMD+RVL configuration root for implicit state. Ambient witness appends resolve in this order:

1. `EPISTEMIC_WITNESS` when explicitly set by the operator.
2. `~/.cmdrvl/state/witness/witness.jsonl` as the default managed state path.

On first default use, `canon` copy-migrates an existing legacy `~/.epistemic/witness.jsonl` or `.epistemic/witness.jsonl` ledger into the canonical path. It never deletes or moves legacy files. Migration and deprecation notices are path-only JSONL records under `~/.cmdrvl/migrations/applied.jsonl` and `~/.cmdrvl/notices/deprecated-paths.jsonl`; file contents and secret values are never recorded.

### Registry maintenance subcommands

`canon` also exposes explicit registry-maintenance workflows that reuse the normal exact-match parser and lookup semantics without changing the `canon.v0` resolution contract.

`canon registry build --source <SOURCE> --seed <SEED> --seed-column <COLUMN> --output <DIR> --version <VER> [--incremental] [--max-rows <N>] [--max-bytes <N>] [--batch-size <N>] [--rate-limit-ms <MS>] [--provider-config <KEY=VALUE>]`
- materializes a standard registry directory from a provider-backed seed corpus using the same dedup semantics as normal resolution
- writes `registry.json`, mapping files, and `_build.json` provenance
- exits `0` on successful materialization, `2` on refusal
- partial provider failures are preserved in the JSON report and warned on stderr; successful mappings still land in the registry directory
- `--provider-config` is repeatable and carries provider-specific options such as OpenFIGI `id_type`, `base_url`, or corpus-wide mapping filters such as `exchCode=US`
- OpenFIGI is a corpus-scoped securities identifier provider for CUSIP, ISIN, and SEDOL seeds; it is not CMBS-specific and is not an organization identity source
- OpenFIGI mapping filters are passed through only during provider-backed materialization. Supported static filter keys are `exchCode`, `micCode`, `currency`, `marketSecDes`, `securityType`, `securityType2`, `optionType`, `includeUnlistedEquities`, `strike`, `contractSize`, `coupon`, `expiration`, and `maturity`; all non-secret filter values are recorded in `_build.json`.
- identifier-heavy OpenFIGI workflows should extract identifiers from source tapes, normalize/dedupe them, split CUSIP/ISIN/SEDOL into separate seed files, run one build per id type, publish static registries, and use `--incremental` for refreshes
- provider implementation and regression tests should use a local `twinning rest` fixture through `--provider-config base_url=...`; live OpenFIGI calls are maintenance operations, never normal lookup behavior

`canon registry export --format dbt-seed|search-index --registry <REGISTRY> --out <PATH> [--namespace <CONTEXT>] [--source-file <FILE>...] [--canonical-type <TYPE>...] [--rule-id-prefix <PREFIX>...] [--canonical-iri-prefix <PREFIX>] [--schema-out <schema.yml>] [--anti-collapse-test-out <test.sql>] [--emit json|summary]`
- exports a versioned flat registry into a downstream artifact without changing normal lookup semantics or mutating registry files
- applies explicit narrowing filters before deterministic first-match deduplication; `--namespace` is required for `dbt-seed` so transform consumers cannot accidentally treat a full shared registry as one context
- `dbt-seed` writes a deterministic CSV with `namespace`, `source_input`, `normalized_key`, `canonical_id`, `canonical_iri`, `canonical_type`, `alias_kind`, `rule_id`, `match_source`, registry provenance, source file, and entry order
- `dbt-seed` can also write a companion dbt `schema.yml` and an anti-collapse singular test asserting one `(namespace, normalized_key)` does not map to multiple canonical IDs
- `search-index` writes a self-describing SQLite artifact with `metadata`, `entities`, `aliases`, `external_keys`, `scoring_tiers`, `field_weights`, `alias_kind_weights`, and FTS5-backed `aliases_fts`
- `canonical_iri` is generated by prefixing bare canonical IDs with `--canonical-iri-prefix` (default `cmdrvl:`); IDs that already look like IRIs or URLs are preserved
- the embedded normalization spec is generic (`canon_registry_search_key.v0`): ASCII uppercase, then remove every non-ASCII-alphanumeric character
- emits `canon_registry_export.v0` JSON or a human-readable summary; exits `0` on successful export, `2` on refusal

`canon registry providers [--emit json|summary]`
- lists the registry build providers available for materialization with their id, name, description, and supported seed columns
- emits `canon_registry_providers.v0` JSON (default) or a human-readable summary; deterministic and offline (no provider call)
- the same catalog is exposed under the top-level `providers` key in `canon --describe`/`operator.json`, and a test guards the two against drift

`canon registry provider-schema <PROVIDER> [--emit json|summary]`
- emits the machine-discoverable `--provider-config` option contract for one provider as `canon_registry_provider_schema.v0`: option keys, value types (`string`, `bool`, `enum`, `url`, `numeric_interval`, `date_interval`), required status, secret flags, environment fallbacks, defaults, examples, mutual exclusions, and the interval encoding rule
- `--provider-config` is generic transport at the CLI boundary; each provider owns and publishes its option schema. The allowed keys, secret status, and validation rules are therefore discoverable through canon, not only through prose or source, so skills and agents read the schema instead of hard-coding option lists
- secret options (e.g. OpenFIGI `api_key`) are flagged `"secret": true` so agents do not echo their values; this matches `_build.json` redaction
- the OpenFIGI schema is derived from the same constants the resolver validates against, so the published contract cannot drift from runtime behavior; tests assert this and never contact `api.openfigi.com`
- an unknown provider refuses with `E_PARSE`, the available provider ids in `detail`, and `next_command: canon registry providers --emit json`; deterministic and offline

`canon registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain|json]`
- inspects existing `canonical_id` values in all root mapping files and suggests the next `<PREFIX>-<number>` ID
- is read-only; it does not write registry files or mutate the derived SQLite index
- uses `registry.json.default_id_scheme` when `PREFIX` is omitted; without either source, refuses with recovery guidance
- treats non-numeric in-namespace canonical IDs as malformed registry state instead of skipping them
- emits plain text by default (`<PREFIX>-<zero-padded-number>`) or `canon_registry_next_id.v0` JSON
- exits `0` when an ID is suggested, `2` on refusal

`canon registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>] [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]`
- appends one exact alias entry to an existing root-level mapping file; `--alias-file` cannot point into a subdirectory and cannot be `registry.json` or `_build.json`
- requires `--input` to be non-empty and already ASCII-trimmed; the lookup key written is exactly that value
- refuses duplicate inputs already present anywhere in the registry because first-match precedence would make the new entry ambiguous or shadowed
- infers `--canonical-type` only when the canonical ID already exists with exactly one type; new canonical IDs require an explicit type
- defaults to a patch semver bump when `--next-version` is absent; use `--next-version` for calendar or other non-numeric versions
- updates `registry.json.version` and `registry.json.entry_count`, runs standard lint by default, and restores the original files if lint reports errors
- emits `canon_registry_add_entry.v0` JSON by default; `--emit plain` prints a one-line shell-oriented receipt
- exits `0` on accepted write, `2` on refusal

`canon registry mint --registry <DIR> [--canonical-id <ID> | --prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>... [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]`
- creates one self-authored canonical ID with one or more starting exact aliases in a single versioned write
- accepts either an explicit `--canonical-id` or an allocation prefix; when both are omitted, allocation uses `registry.json.default_id_scheme`
- requires at least one `--with-alias`; each alias target file must already exist at the registry root
- parses each alias at the first `=` and the last `:`, so alias inputs may contain colons when quoted by the shell
- applies one version bump and one `entry_count` increment covering all added aliases, then runs the same standard lint gate as `add-entry` unless `--no-lint` is set
- emits `canon_registry_mint.v0` JSON by default; `--emit plain` prints only the minted canonical ID
- exits `0` on accepted write, `2` on refusal

`canon registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict] [--bump patch|minor|major | --next-version <VER>] [--emit json|plain]`
- persists the registry's default self-authored ID convention in `registry.json.default_id_scheme`
- validates prefixes as uppercase ASCII letters/digits starting with a letter; default `--zero-pad` is `3`, with allowed range `1..=20`
- is metadata-only: it writes `registry.json` and does not change mapping entries, `_build.json`, or `_index.sqlite`
- warns about existing in-namespace canonical IDs that do not conform; `--strict` turns those warnings into a refusal
- defaults to a patch semver bump when `--next-version` is absent
- emits `canon_registry_default_id_scheme.v0` JSON by default; `--emit plain` prints `<PREFIX>-<zero_pad>`
- exits `0` on accepted write, `2` on refusal

`canon registry diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]`
- compares two versions of the same registry id
- emits `canon_registry_diff.v0` JSON or a human-readable summary line
- exits `0` on successful comparison, `2` on refusal

`canon registry audit <SEED> --registry <REGISTRY> --column <COLUMN> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]`
- audits a seed corpus against a registry using the same dedup + exact-match semantics as normal resolution
- emits `canon_registry_audit.v0` JSON with `resolved`, `unresolved`, `canonical_targets`, and `rule_hits`, or a human-readable summary line
- exits `0` on successful audit, `2` on refusal
- does not change the normal `canon.v0` output contract or witness semantics for the primary resolution path

`canon registry lint <REGISTRY> [--profile standard|org|strategy|auto] [--emit json|summary]`
- validates a registry directory without mutating lookup indexes, mappings, strategy entries, or profile-specific entity sidecars
- emits `canon_registry_lint.v0` with severity-tagged findings, counts by category, registry provenance, and next-command guidance
- `standard` checks `registry.json`, mapping-file parseability, duplicate/shadowed inputs, stale `entry_count`, empty required mapping fields, and lookup-index rebuild eligibility
- `strategy` checks `_strategy` entries, recomputed `schema_fingerprint`, proof hash/reference presence, duplicate `(schema_fingerprint, skill_hash)` keys, stale `entry_count`, and script metadata completeness
- `org` checks alias files, trusted-anchor sidecars, escrow sidecars, lookup/escrow snapshot-hash inputs, malformed records, and conflicting aliases/anchors/escrow records
- `auto` chooses `strategy` when `_strategy/` exists, `org` when entity sidecars exist, and `standard` otherwise
- exits `0` when the lint report is emitted, even if findings are present; exits `2` only on refusal

### Strategy registry subcommands

`canon strategy` extends the same registry discipline to deterministic script reuse. It resolves a typed strategy key plus skill hash to a frozen script pointer; it does not execute scripts. Strategy entries use `entry_schema_version: "canon_strategy_entry.v1"`, a typed `key` (`schema` or `task`), `grade` (`operator-attested` or `proof-attested`), and `status` (`active` or `deprecated`). Legacy schema/proof entries load as schema-keyed, proof-attested, active entries.

`canon strategy profile <INPUT> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]`
- reads CSV, TSV, JSONL, or NDJSON rows using the same format detection and max-limit refusal patterns as the normal identifier path
- emits `canon_strategy_profile.v0`
- sorts canonicalized top-level columns deterministically by name
- records primitive type labels, exact distinct cardinalities, value/null/empty/missing/non-scalar counts, raw input byte count, raw input BLAKE3 hash, schema fingerprint, and profile content hash
- emits a top-level `columns` array that can be passed directly to `canon strategy resolve --schema` or `canon strategy register --schema`
- exits `0` on successful profiling and `2` on refusal

`canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR> [--emit json|summary]`
- parses the same schema/profile shape accepted by `resolve` and `register`
- hashes the script bytes, schema shape, and fixture suite inputs/expected outputs
- reads `<DIR>/manifest.json` with `suite_id`, optional `version`, optional `repeatability_runs`, and fixtures containing `id`, `input`, `expected_stdout`, and optional `expected_exit_code`
- executes the script once per fixture with fixture input bytes on stdin, compares stdout and exit code to expected outputs, then repeats the run to refuse nondeterministic output
- emits `canon_strategy_audit.v0` with script hash, schema fingerprint, suite hash, deterministic output hash, fixture pass/fail metrics, and a gate decision
- a passing artifact includes `passed: true`, `decision: "PROCEED"`, `sealed: true`, `status: "PASS"`, and `result: "SUCCESS"` so the same artifact can satisfy `strategy register`'s `--verify`, `--assess`, and `--airlock` gates
- exits `0` when all deterministic fixture checks pass, `1` when deterministic fixture checks fail, and `2` on refusal
- refuses malformed suites or nondeterministic script outputs with structured refusal envelopes

`canon strategy resolve --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]`
- reads `registry.json` and `_strategy/*.json` entries from a local versioned registry
- hashes `--skill` with BLAKE3, unless `--skill-hash` is provided directly
- for `--schema`, parses a JSON schema/profile artifact with top-level `columns` or `fields`
- for `--task`, ASCII-trims the task and performs exact active-key lookup only; no fuzzy aliases, schema tiers, or normalization are applied
- emits `canon_strategy_resolve.v0`
- schema keys exit `0` for `EXACT` or `COMPATIBLE`, `1` for `PARTIAL` or `UNRESOLVED`, and `2` on refusal
- task keys exit `0` for `EXACT`, `1` for `UNRESOLVED`, and `2` on refusal; `COMPATIBLE` and `PARTIAL` are not task outcomes

Schema resolution tiers:
- `EXACT`: identical column names, types, and cardinalities for the same skill hash; run the frozen script
- `COMPATIBLE`: identical column names and types but different cardinalities; run the frozen script
- `PARTIAL`: overlapping columns with missing, extra, or type-changed fields; escalate for rewrite
- `UNRESOLVED`: no same-skill schema overlap; author a new script, gate it, then register it

`canon strategy register --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --grade operator-attested|proof-attested --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--rule-id <RULE>] [--emit json|summary] [--no-witness]`
- refuses unless `--next-version` differs from `registry.json`
- refuses duplicate active typed keys; deprecated entries are preserved but ignored by active resolution
- `operator-attested` requires `--operator`, single-line `--reason`, timestamp, skill/script hashes, and records an operator attestation hash; it does not require verify/assess/airlock
- `proof-attested` requires verify status `PASS`/`PASSED`/`SUCCESS` or `passed:true`, assess decision `PROCEED`, and airlock status `PASS`/`PASSED`/`SEALED`/`SUCCESS` or `sealed:true`
- appends a registry entry under `_strategy/entries.json`, records BLAKE3 hashes for schema or task, skill/script bytes, attestation/proof artifacts, and updates `registry.json` version + entry_count
- emits a deterministic mutation receipt with operation, before/after version, before/after registry hash, typed key, grade/status, script hash, source file, entry order, and next-command hints
- appends a witness record for successful mutations unless `--no-witness` is passed; witness records contain receipt/provenance hashes, not raw script bytes or secrets

Strategy registries are local artifacts. No remote provider calls happen during `profile`, `resolve`, `register`, or `diff`.

`canon strategy update --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--emit json|summary] [--no-witness]`
- updates exactly one active entry in place, preserving key identity and registry history
- operator-attested updates refresh operator attestation provenance; proof-attested updates require fresh proof artifacts
- emits the same mutation receipt and witness behavior as register

`canon strategy deprecate --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER> [--attested-at <RFC3339>] [--emit json|summary] [--no-witness]`
- marks exactly one active entry `deprecated` and records deprecation provenance
- never physically deletes registry entries
- active resolution ignores deprecated entries; `explain` still shows them as ignored history
- emits the same mutation receipt and witness behavior as register

`canon strategy promote --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json> --next-version <VER> [--emit json|summary] [--no-witness]`
- promotes exactly one active operator-attested entry to proof-attested by attaching proof references
- refuses already proof-attested entries or invalid proof artifacts
- emits the same mutation receipt and witness behavior as register

`canon strategy list --registry <REGISTRY> [--key-type schema|task] [--grade operator-attested|proof-attested] [--status active|deprecated] [--emit json|summary]`
- emits `canon_strategy_list.v0` with typed key, skill hash, grade, status, script metadata, provenance summary, source file, and entry order
- is read-only and does not append witness records

`canon strategy explain --registry <REGISTRY> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]`
- emits `canon_strategy_explain.v0` describing the active entry, ignored deprecated entries, and the next command for resolve or register
- is read-only and does not append witness records

`canon strategy diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]`
- compares two strategy registry versions with the same registry id
- emits `canon_strategy_diff.v0`
- keys effective entries by typed key plus skill hash
- reports `added`, `removed`, `changed`, and `unchanged` entries
- classifies changes by script id/path/language/content hash, proof hashes, operator attestation hash, schema shape, grade, status, typed key shape, and rule id
- resolves duplicate keys deterministically by filename-sorted, entry-order precedence; shadowed duplicates do not affect the effective diff
- exits `0` on successful comparison and `2` on refusal

### Entity workbench subcommands

`canon entity` is the native evidence-to-registry workbench. It is not the core
lookup path and it does not change `canon.v0` exact-match semantics.

- **Cluster mode**: `canon entity run` groups profiled observations inside one
  corpus through prepare, index, block, evidence, solve, audit, review, and
  promotion artifacts.
- **Link mode**: `canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--suite <DIR>] [--gold <GOLD.jsonl>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]`
  aligns two row sets through the same typed request and artifact path as
  project mode. It is not a public `edge` alias, does not bypass
  evidence/audit/review, and emits `canon_entity_link.v1` with deterministic
  `canon_entity_link_decisions.v1`, `canon_entity_link_observation_surface_bindings.v1`,
  a hash-bound `profile_source`, and any published assignment-alignment
  artifacts marked as nonidentity relation hints rather than issuer/entity
  identity.
  `--write-back` is accepted by the parser only as a structured handoff: the
  active v1 public path refuses it before work-dir or registry mutation until
  transactional registry publication is available through review/promote/apply.

  `--profile` and `--work-dir` stay bracketed where this document mirrors
  generated Clap help/operator usage because the parser emits a structured
  `E_ENTITY_INPUT_CONTRACT` refusal when either is omitted. Successful link
  execution requires both flags, and omission performs no writes.

`canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json> [--emit json|summary]`
- compiles a strict alias-withholding execution envelope into a benchmark report
- the envelope version is `canon.evaluation.alias_withholding.execution_manifest.v0`
  and contains the benchmark plus one native execution manifest per trial
- loads manifest-relative artifacts for clean registry absence, candidate
  recall, link, run/solve, review, audit, leak scans, assignment firewall, and
  optional reviewed promotion plus exact replay
- requires the real clean registry id/version and complete exact mapping set to
  equal the benchmark retained snapshot; absence against a weaker or unrelated
  registry is not evidence
- recomputes nonempty source-bound scans for mapping, search-index, cache,
  normalization-patch, generated-corpus, and display-name channels; the mapping
  scan enumerates the complete clean registry tree, and the assignment firewall
  checks distinct concrete assignment and issuer-identity sources
- for a credited attachment, rebuilds a cluster-mode review queue from the
  validated solve artifact, binds a typed native review-import receipt to that
  queue/run/policy/registry context, requires an exact one-entry clean-to-promoted
  registry diff, and verifies ordinary exact replay against the promoted registry
- keeps resolver/link observation IDs separate from prepared surface IDs through
  a hash-bound sidecar that is re-derived from materialized rows and the supplied
  run profile/strategy before candidate, solve, review, or promotion joins
- reports candidate disposition explicitly: `evaluated_pair` requires rank-or-miss
  evidence; `prepared_surface_collapse` binds distinct link observations to one
  prepared surface without candidate-rank/recall credit; `relation_policy_control`
  excludes non-identity controls from recall, forbids promotion/replay, and records
  an automatic attachment as an `unsupported_guess` false merge
- supports a derivation-proven collapse receipt through the public native review
  artifact/import path: a singleton cluster Alias decision requires an explicit
  target canonical ID and exact exported-surface equality
- derives trial decisions, promotion replay, and aggregate counts from those
  artifacts; caller-declared outcomes or self-reported pass/fail fields are not
  accepted as evidence
- emits `json` by default or a compact `summary`; a valid report exits `0`, and
  malformed envelopes or stale/missing/tampered artifacts refuse with exit `2`
- hashes report identifiers and manifest paths at the CLI boundary; public
  refusals expose a stable reason and message fingerprint rather than private
  artifact text
- does not mutate registries and does not change normal exact lookup semantics

`canon entity generalization --manifest <STRICT_ENVELOPE.json> [--emit json|summary]`
- compiles a strict artifact-backed entity-disjoint/time-forward execution
  envelope into a benchmark report
- uses the same public command for public fixtures and operator-owned private
  corpora
- loads manifest-relative native candidate-recall, link, run, solve,
  observation/surface sidecar, and leakage-source artifacts by path, version,
  and content hash
- requires strict solve derivation refs for `solve_derivation.edge_artifact.path`,
  `solve_derivation.edge_records.path`, `solve_derivation.prepared_surfaces.path`,
  and a hash-bound `solve_derivation.solve_policy` artifact with version
  `canon.evaluation.generalization.solve_policy.v0`; the policy file byte hash
  must equal `cross_bindings.policy_digest`, and edge/prepared refs are
  path-bound to the loaded run `work_dir`
- requires each trial's `registry_dir` to be manifest-relative and resolved
  inside the envelope root; absolute paths, traversal segments, and symlink
  registry roots are refused
- treats `run.metadata.registry_snapshot.source` as inert metadata continuity,
  not as a filesystem path to open
- rebuilds the loaded solve and run exactly from those derivation inputs before
  scoring
- derives decisions, candidate ranks, false-merge outcomes, and leakage status
  from those artifacts rather than self-attested fields in the envelope
- emits main report version `canon.evaluation.generalization.v1` with nested
  `quality.version` `canon.evaluation.generalization.quality_gate_report.v0`,
  `quality.contract_version` `canon.entity.quality.v1`, fixed canonical gate
  results for `candidate_recall_at_50_min`, `auto_link_precision_min`,
  `auto_link_recall_min`, `critical_false_merges_max`, and
  `accounted_case_rate_min`, and `quality.release_claim_status`
  `eligible|blocked`
- defines the `canon.entity.quality.v1` fixed thresholds as
  `candidate_recall_at_50 >= 0.995`, `auto_link_precision >= 0.995`,
  `auto_link_recall >= 0.98`, critical false merges `== 0`, and
  `accounted_case_rate == 1.0`; zero-denominator rate gates emit
  `not_applicable`, which keeps `quality.release_claim_status` `blocked`
  because eligibility requires every gate to pass
- accepts no caller-adjustable threshold or waiver inputs for this command
- hashes report identifiers, paths, and cutoffs at the CLI boundary; public
  refusals expose a stable reason and message fingerprint rather than private
  artifact text
- is read-only, exits `0` for a structurally valid `eligible` or `blocked`
  report, including low-quality or critical-false-merge blocked reports, and
  exits `2` only for malformed envelopes or stale/missing/tampered artifacts;
  it does not change normal exact lookup semantics

`canon entity prepare <ROWS> --profile <PROFILE> --registry <REGISTRY> --work-dir <DIR>`
- validates the profile, registry snapshot, and source rows
- writes prepared surfaces and profile firewall artifacts under the work
  directory
- emits `canon_entity_prepare.v1`-family artifacts and refuses malformed profile
  or stale registry inputs before downstream stages run

`canon entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--emit json|summary]`
- builds deterministic local index artifacts for candidate generation
- records profile, strategy, registry, and source hashes for cache correctness
- records native cache receipts as `canon_entity_index_cache_receipt.v0`;
  `canon entity run` and `canon entity link` expose
  `--cache-mode enabled|disabled` and default to `enabled`
- a genuine warm hit requires an enabled reusable receipt and reports
  `cache_enabled` with status `hit`, while disabled cache mode bypasses reuse,
  writes a non-reusable
  `cache_disabled` receipt, and reports status `bypassed`
- hashes the complete cache bundle in the receipt: index artifact, cache key,
  postings, and diagnostics bytes
- binds cache receipts to the native index stage, refuses stale/tampered bundles
  or invalid mode/status/reusable triples before reuse, and preserves semantic
  output parity between enabled and disabled cache modes

`canon entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <REGISTRY> [--work-dir <DIR>] [--emit jsonl|summary]`
- emits bounded candidate neighborhoods from configured blocking operators
- preserves candidate-budget diagnostics and refuses over-budget runs before
  writing partial candidate artifacts

`canon entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <REGISTRY> [--work-dir <DIR>] [--emit jsonl|summary]`
- scores typed evidence for blocked candidate pairs
- preserves support, anti-merge, and relation-hint lanes separately
- relationship evidence is not equivalence evidence unless a separate
  profile-approved equality signal supports the merge

`canon entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <REGISTRY> [--work-dir <DIR>] [--emit json|summary]`
- solves deterministic same-entity assignments from evidence artifacts
- abstains into review or escrow on ambiguous, contradictory, stale, or
  insufficient evidence

`canon entity apply <RESULT.json> --rows <ROWS> --registry <REGISTRY> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution|--allow-partial-output] [--emit json|summary]`
- replays accepted assignments from a solve/run artifact onto input rows
- does not mutate the registry

### Entity review subcommands

`canon entity review export <RESULT.json> [--artifact queue|native-review] [--emit json|csv|html] [--include resolved|escrow|contradictions|all]`
- defaults to `--artifact queue`, preserving the existing queue/v1 and legacy
  review export behavior and `canon_entity_review_export.v0`
- `--artifact native-review` emits `canon_entity_native_review.v0` for native
  solve, run, or link artifacts; `--emit html` is valid only for this explicit
  native artifact path
- includes deterministic review IDs, source row IDs, observed names, anchors,
  incumbent overlaps, evidence scores, contradiction reasons, and proposed
  review actions
- `--include` filters to resolved/promotable entities, escrowed abstentions,
  contradiction records, or all reviewable items; default is `all`
- native review artifacts carry an artifact self-hash, run/policy/registry
  binding, and exact cluster/link mode context; candidate-free unmatched
  directional links carry `right_surface_id: null` and defer-only allowed actions
- native JSON and CSV exports are deterministic decision envelopes, and native
  HTML is a static offline projection of that artifact

`canon entity review import <REVIEW.json|csv> --registry <REGISTRY> --next-version <VER> [--audit <AUDIT.json>] [--source-review <NATIVE_REVIEW.json>] [--emit json|summary]`
- without `--source-review`, imports the existing queue review decisions into
  alias, trusted-anchor, pending-escrow, and cannot-link sidecars, then bumps
  `registry.json` to `--next-version`
- with `--source-review <canon_entity_native_review.v0>`, treats the positional
  `<REVIEW>` file as native JSON/CSV decisions and emits
  a typed `canon_entity_native_review_import.v0` patch receipt only; native
  import does not read or mutate the registry and does not consume `--audit` or
  `--next-version` beyond Clap-required compatibility
- default queue import uses the required `--registry` and `--next-version`
  arguments; `--audit` remains required for default alias/anchor promotion
  decisions
- default queue import refuses malformed decisions, duplicate review IDs, stale
  registry snapshots, alias overwrites, trusted-anchor conflicts, and unchanged
  or empty next versions; native import refuses malformed/duplicate decisions,
  source-review self-hash mismatches, and exact mode-context mismatches
- default import emits `canon_entity_review_import.v0` with registry before/after
  hashes, write counts, and BLAKE3 proof hashes for review input, optional audit
  input, alias patch, anchor patch, and escrow patch

### Output modes

| `--emit` | stdout | Mapping artifact | Use case |
|----------|--------|------------------|----------|
| `json` (default) | JSON mapping object | IS stdout | Audit, pack, inspection |
| `csv` | Canonicalized CSV with `<col>__canon` appended | Written to `--map-out` if specified | Pipeline stage — feed directly to `rvl`, `verify`, etc. |

In `json` mode, `canon` is an artifact tool (always structured JSON on stdout). In `csv` mode, `canon` becomes a pipeline stage (file in, file out) with the mapping artifact as an optional sidecar.

Exit codes (core and report commands)
- `0`: success, such as RESOLVED, healthy report, or successful report/write
- `1`: domain outcome needs inspection, such as PARTIAL, UNRESOLVED, unhealthy contract health, or another command-specific failed gate
- `2`: REFUSAL / error

For core resolution, exit codes are the same in both emit modes. In `csv` mode, a PARTIAL result (exit 1) still writes the CSV — unresolved rows have an empty canonical column. The exit code tells you whether to trust it blindly or inspect.

Streams
- `--emit json`: single JSON object to stdout (including refusals — the `"outcome": "REFUSAL"` object IS the stdout output, same pattern as `rvl --json`).
- `--emit csv`: canonicalized CSV to stdout. On refusal (exit 2), no CSV is written to stdout; the refusal JSON object goes to stderr instead.
- stderr is always reserved for process-level warnings, witness failures, and (in csv mode only) refusals.

---

## Outcomes (exactly one)
1) RESOLVED
- every input value mapped to a canonical ID
- all mappings are deterministic with registry version recorded
- `summary.resolved > 0 && summary.unresolved == 0`

2) PARTIAL
- at least one input resolved AND at least one unresolved
- unresolved entries listed with reason
- resolved mappings are still valid — partial is not a failure, it's an honest report
- `summary.resolved > 0 && summary.unresolved > 0`

3) UNRESOLVED
- zero inputs could be mapped
- `summary.resolved == 0 && summary.unresolved > 0`
- this is distinct from REFUSAL — the tool operated correctly, it just found no matches in the registry

4) REFUSAL
- cannot operate (bad input, bad registry, missing column, etc.)

No other outcomes.

Workbench evaluation commands such as `canon entity alias-withholding` and
`canon entity generalization` are report compilers rather than core lookup runs:
a valid report exits `0` even when its internal release claim is blocked, and a
bad execution envelope or referenced artifact exits `2` with a structured
refusal.

---

## Definitions (v0)
- **Input value**: the raw cell content in the specified column, after ASCII-trim
- **Canonical ID**: the resolved identifier from the registry
- **Canonical type**: the type/namespace of the canonical ID (e.g., `ticker`, `isin`, `entity_id`, `property_id`)
- **Rule ID**: the specific mapping rule that produced the match (e.g., `CUSIP_TO_TICKER`)
- **Registry**: a versioned directory of lookup data (see Registry Format below)
- **Deterministic match**: exact lookup in a versioned registry — same input + same registry version = same output, every time
- **Suggested match**: probabilistic or fuzzy match — flagged as `"confidence": "suggested"` and **not accepted** until explicitly persisted to the registry by a human
- **Unresolved entry**: an input value that could not be mapped to a single canonical ID

---

## Input Contract

### CSV input
- Same byte-oriented CSV rules as `rvl`: header required, UTF-8 BOM stripped, delimiter auto-detected (same algorithm)
- `--column` specifies the column containing IDs to resolve
- Column must exist in the input (else REFUSAL `E_COLUMN_NOT_FOUND`)
- All rows are processed; blank records (all fields empty after ASCII-trim) are skipped
- Input values are ASCII-trimmed before lookup. If a value is empty after trim (but the row is not a blank record), it is classified as unresolved with reason `"empty_value"` without performing a registry lookup — an empty string is never a valid identifier. This is distinct from `"no matching rule"` which means a non-empty value had no registry entry.

### JSONL input
- One JSON object per line
- `--column` specifies the field name containing IDs to resolve
- Field must exist in each object (missing field on a line => that line is unresolved with reason `"missing_field"`)
- Input values are string-coerced and ASCII-trimmed before lookup. JSON `null` is treated as a missing value (unresolved with reason `"null_value"`), not coerced to the string `"null"`. Objects and arrays are unresolved with reason `"non_scalar_value"` (IDs are scalars; structured values aren't coercible to a lookup key). Numbers and booleans are coerced to their JSON string representation (e.g., `42` → `"42"`, `true` → `"true"`).

### Identifier encoding
- Input values and canonical IDs in JSON output use the same encoding as `rvl`:
  - `u8:<utf8-string>` if valid UTF-8 with no ASCII control bytes
  - `hex:<lowercase-hex-bytes>` otherwise

---

## Registry Format

A registry is a versioned directory of JSON mapping files.

```
registries/cusip-isin/
+-- registry.json            # Metadata: id, version, description, updated
+-- cusip-to-isin.json       # Mapping file: array of { input, canonical_id, canonical_type, rule_id }
+-- cusip-to-ticker.json     # Mapping file
```

### `registry.json` schema
```json
{
  "id": "cusip-isin",
  "version": "3.2.1",
  "description": "CUSIP to ISIN and ticker mappings",
  "updated": "2026-01-15",
  "entry_count": 48291
}
```

`default_id_scheme` is optional metadata for self-authored registry maintenance. Existing registries without it remain valid. When present, `canon registry next-id` and `canon registry mint` can allocate IDs without a prefix argument, and `canon registry add-entry` validates explicit canonical IDs against the convention.

```json
{
  "default_id_scheme": {
    "prefix": "PPL",
    "zero_pad": 3
  }
}
```

### Mapping file discovery
- All `*.json` files in the registry directory except `registry.json` and `_build.json` are treated as mapping files
- Subdirectories are ignored (flat structure only in v0)
- Non-JSON files (e.g., `.md`, `.txt`) are ignored
- If a discovered `.json` file is not a valid mapping file (wrong schema, malformed JSON) → REFUSAL `E_BAD_REGISTRY`
- Files are evaluated in filename-sorted (lexicographic) order for match precedence

### Mapping file schema (each entry)
```json
{
  "input": "037833100",
  "canonical_id": "AAPL",
  "canonical_type": "ticker",
  "rule_id": "CUSIP_TO_TICKER"
}
```

### Registry types

Registries vary in complexity. `canon` treats all registries uniformly — input values in, canonical IDs out, unresolved entries flagged — but internal structure differs by domain:

| Registry type | Matching | v0? | Example |
|---------------|----------|-----|---------|
| **ID mapping** | Exact lookup (input ID -> canonical ID) | Yes | CUSIP->ISIN, ticker normalization |
| **Alias resolution** | Exact lookup with pre-populated variants | Yes | "Alpha Entity LLC" / "Alpha Entity" -> entity ENT-00012 (each variant is a separate registry entry) |
| **Entity workbench** | Multi-field deterministic evidence outside the lookup path, promoted into flat aliases and sidecars | Yes, via `canon entity` | "Alpha Entity Holdings" / "Alpha Entity" -> entity ENT-00012 |
| **Cross-source linkage** | Two-row-set deterministic matching under an explicit strategy, promoted into flat registry entries | Yes, via `canon entity link` | Reference row `R-223` -> target row `T-771` |
| **Property/address identity** | Address/geospatial/name evidence under a property-specific strategy | Future | Property address variants -> canonical property P-00456 |

ID mapping and alias resolution both use the same v0 lookup mechanism: exact
byte match after ASCII-trim. The difference is how the registry is authored
(one entry per ID vs many entries per entity). `canon entity` cluster and link
modes are not new lookup match modes; they are workbench modes that use
deterministic evidence to manufacture audited registry updates. Property-specific
identity remains future work.

### Registry creation patterns

Registry creation has two supported operational shapes:

- **Provider-fetched:** `canon registry build` consumes a seed corpus and writes a normal registry directory plus `_build.json` provenance. Provider calls happen only during maintenance; normal lookup never calls providers. OpenFIGI-backed builds materialize corpus-scoped CUSIP, ISIN, or SEDOL mappings into ordinary registry files and do not become a provider-backed resolver.
- **Self-authored:** operators create canonical entities by convention using `canon registry default-id-scheme`, `next-id`, `mint`, and `add-entry`. The durable product is still flat mapping entries that exact lookup can resolve.

Self-authored registry maintenance is not a resolution workbench. It does not score candidates, inspect multiple columns, or infer that two observations represent the same entity. It records an operator's accepted alias decision as deterministic registry data.

### Registry governance

- Registry content is separate from the engine. Example, demo, private, or
  commercial registry packages are still ordinary versioned registries that the
  operator references with `--registry <PATH>`.
- Canon core does not contain special built-in resolution for sectors, deals,
  servicers, providers, or ontology terms. Those choices belong in registry
  packages, profiles, provider materializers, or extensions.

### Versioning

- Registries are versioned at the directory level (`registry.json` carries the version)
- Follows semver: any entry addition, modification, or removal bumps the version
- The `registry.json` version is recorded in `canon` output for reproducibility
- Directories are inspectable, diffable, and versionable in git
- `entry_count` in `registry.json` is advisory (for display/logging) — `canon` does not refuse on mismatch with actual entry count, but logs a warning to stderr if they differ

---

## Lookup Behavior

### Resolution order
1. Load registry from `--registry` path
2. Validate registry format (refuse on malformed registry)
3. Parse input file, extract `--column` values
4. For each unique input value (after ASCII-trim and special-case classification):
   - If value is empty string → unresolved with reason `"empty_value"` (no lookup)
   - If value originated from JSONL `null` → unresolved with reason `"null_value"` (no lookup)
   - If value originated from a missing JSONL field → unresolved with reason `"missing_field"` (no lookup)
   - If value originated from a JSONL object or array → unresolved with reason `"non_scalar_value"` (no lookup)
   - Otherwise: look up in registry via exact byte match against `input` fields
   - If match: record mapping (input, canonical_id, canonical_type, rule_id, confidence)
   - If no match: record as unresolved with reason `"no matching rule"`

### Match precedence
- Exact match (byte-for-byte after ASCII-trim) takes priority
- Within a registry, mapping files are evaluated in filename-sorted order
- First match wins (no ambiguity — if two rules could match, the first one by file order + entry order is used)

### Normalization and alias matching (v0)
Alias resolution in v0 is **not fuzzy** — it is exact lookup against pre-normalized registry entries. The registry author is responsible for including all known variants as separate `input` entries:

```json
{"input": "Wells Fargo", "canonical_id": "C-00012", ...}
{"input": "Wells Fargo Bank, N.A.", "canonical_id": "C-00012", ...}
{"input": "WFB", "canonical_id": "C-00012", ...}
```

`canon` does not normalize input values beyond ASCII-trim. No uppercasing, no punctuation stripping, no stemming. This keeps the matching fully deterministic and transparent — the registry is the complete source of truth for what matches.

If a registry needs case-insensitive matching, it must include all case variants as entries (or a future `canon` version may support a per-registry `match_mode` field in `registry.json` — see v1 ideas).

> **Rationale:** Implicit normalization rules are a common source of subtle bugs in entity resolution systems. By keeping v0 matching purely exact (post-ASCII-trim), every resolution is directly traceable to a specific registry entry. The registry is auditable; the matching is trivial.

### Suggestions vs accepted mappings
- Suggestions may be probabilistic (e.g., fuzzy name matching), but **accepted mappings are deterministic, persisted, and versioned**
- v0 only supports deterministic matching (exact lookup + alias resolution)
- Probabilistic suggestions (via `canon suggest` mode) are deferred to v1

### Duplicate input values
- Input values are deduplicated before lookup — each unique value is resolved once
- `mappings[]` and `unresolved[]` contain one entry per **unique input value**, not one per row
- `summary.total` counts unique input values (after ASCII-trim), not row count
- Row count is not tracked in output (the tool maps values, not rows)
- This keeps output size proportional to cardinality, not file length (500 unique CUSIPs = 500 mapping entries, regardless of whether the file has 500 or 500k rows)

### Unresolved entries and hypotheses
- When `canon` cannot resolve an input to a single canonical ID, it emits the entry as `unresolved` with the reason
- Downstream systems (e.g., the data factory's decode policy) may hold unresolved entries as provisional hypotheses rather than treating them as terminal failures

---

## Output (JSON: `--emit json`, default)

Single JSON object on stdout. This is the default output mode and the format used for `--map-out` in CSV mode.

### Schema (`canon.v0`)
```json
{
  "version": "canon.v0",
  "outcome": "PARTIAL",
  "registry": {
    "id": "cusip-isin",
    "version": "3.2.1",
    "source": "registries/cusip-isin/"
  },
  "summary": {
    "total": 4183,
    "resolved": 4150,
    "unresolved": 33
  },
  "redacted": true,
  "mappings": [
    {
      "input": "u8:037833100",
      "canonical_id": "u8:AAPL",
      "canonical_type": "ticker",
      "rule_id": "CUSIP_TO_TICKER",
      "confidence": "deterministic"
    }
  ],
  "unresolved": [
    {
      "input": "u8:UNKNOWN123",
      "reason": "no matching rule"
    }
  ],
  "refusal": null
}
```

### Field definitions

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Always `"canon.v0"` |
| `outcome` | string | `"RESOLVED"`, `"PARTIAL"`, `"UNRESOLVED"`, or `"REFUSAL"` |
| `registry.id` | string | Registry identifier |
| `registry.version` | string | Registry semver |
| `registry.source` | string | Path to registry directory (as provided via `--registry`; may be relative or absolute — consumers should not assume filesystem semantics) |
| `summary.total` | integer | Count of unique entries processed: unique normal input values (after ASCII-trim, excluding blank records) plus one per distinct special reason that fired (see Dedup rules below) |
| `summary.resolved` | integer | Count of successfully mapped entries |
| `summary.unresolved` | integer | Count of entries that could not be mapped |
| `redacted` | boolean | Present on `RESOLVED`/`PARTIAL`/`UNRESOLVED` outputs. `true` when `input` and `canonical_id` values are masked as `"[REDACTED]"` (the zero-retention default); `false` when `--explicit` reveals them. Absent on `REFUSAL` (no values to mask). A discovery breadcrumb so consumers can detect masking without parsing `--help` |
| `mappings[]` | array | One entry per resolved input |
| `mappings[].input` | string | Original input value. Identifier-encoded by default (`u8:`/`hex:`); plain UTF-8 without `u8:` only when `--explicit --plain-json-values` is used |
| `mappings[].input_encoding` | string | Optional. Present only with `--explicit --plain-json-values`; `"utf8"` for plain text or `"hex"` for `hex:<bytes>` fallback |
| `mappings[].canonical_id` | string | Resolved canonical ID. Identifier-encoded by default (`u8:`/`hex:`); plain UTF-8 without `u8:` only when `--explicit --plain-json-values` is used |
| `mappings[].canonical_id_encoding` | string | Optional. Present only with `--explicit --plain-json-values`; `"utf8"` for plain text or `"hex"` for `hex:<bytes>` fallback |
| `mappings[].canonical_type` | string | Type/namespace of the canonical ID |
| `mappings[].rule_id` | string | Which mapping rule produced this match |
| `mappings[].confidence` | string | `"deterministic"` or `"suggested"` (v0: always deterministic) |
| `unresolved[]` | array | One entry per unresolved input |
| `unresolved[].input` | string\|null | Original input value. Identifier-encoded by default (`u8:`/`hex:`); plain UTF-8 without `u8:` only when `--explicit --plain-json-values` is used. `null` for special reasons (`"empty_value"`, `"null_value"`, `"missing_field"`, `"non_scalar_value"`) |
| `unresolved[].input_encoding` | string | Optional. Present only with `--explicit --plain-json-values` when `input` is not null; `"utf8"` for plain text or `"hex"` for `hex:<bytes>` fallback |
| `unresolved[].reason` | string | Why resolution failed (see reason values below) |
| `refusal` | object/null | Refusal envelope (null unless REFUSAL) |

**Invariant:** `summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved — there is no third bucket.

### Confidence values
- `"deterministic"` — exact match in versioned registry, fully reproducible
- `"suggested"` — probabilistic match, **not accepted** until explicitly persisted to registry (v1)

### Unresolved reason values (v0)

| Reason | Trigger |
|--------|---------|
| `"no matching rule"` | Non-empty input value had no exact match in the registry |
| `"empty_value"` | Input value was empty after ASCII-trim (CSV: non-blank row with blank column; JSONL: empty string field) |
| `"missing_field"` | JSONL object did not contain the `--column` field |
| `"null_value"` | JSONL field value was JSON `null` |
| `"non_scalar_value"` | JSONL field value was an object or array (not coercible to a string ID) |

**Dedup rules for special reasons:** `"empty_value"`, `"null_value"`, `"missing_field"`, and `"non_scalar_value"` each produce at most one unresolved entry regardless of how many input rows triggered them (same dedup principle as regular values). For these entries, `unresolved[].input` is `null` (JSON null, not the string `"null"`) since there is no meaningful input value to report. Each distinct reason that fires contributes 1 to `summary.total` and 1 to `summary.unresolved`.

### Note on output size
The `mappings` and `unresolved` arrays contain one entry per **unique** input value (see Duplicate Input Values above). For registries with high cardinality (e.g., 50k unique CUSIPs), the output JSON can still be large. `canon` emits the complete mapping for pack/audit integrity — agents processing the output should use streaming JSON parsers. The `summary` object provides aggregate counts without reading the full arrays.

---

## Output (CSV: `--emit csv`)

When `--emit csv` is specified, stdout is the original CSV with one column appended: the resolved canonical ID.

### Behavior
- Every row from the input CSV is preserved exactly (same delimiter, same quoting, same field values)
- A new column is appended at the end of each row (header + data)
- Header gets the canonical column name (`<COLUMN>__canon` or `--canon-column` value)
- Resolved rows: the raw `canonical_id` value from the registry match — no identifier encoding prefix (i.e., `AAPL`, not `u8:AAPL`). Identifier encoding (`u8:`/`hex:`) is a JSON output concern only. `canonical_type` and `rule_id` are in the JSON mapping artifact only.
- Unresolved rows: empty string in the new column
- Blank records: passed through unchanged (canonical column is empty). Note: blank rows appear in CSV output but are excluded from the JSON mapping artifact's `summary.total`, `mappings[]`, and `unresolved[]` — the CSV preserves row structure while the JSON counts unique processable values.
- Delimiter matches the input file's detected delimiter
- Quoting follows the input file's detected escape mode

### Example
```
$ cat tape.csv
cusip,balance,rate
037833100,1000000,3.5
594918104,500000,4.2
UNKNOWN99,250000,2.8

$ canon tape.csv --registry registries/cusip-isin/ --column cusip --emit csv
cusip,balance,rate,cusip__canon
037833100,1000000,3.5,AAPL
594918104,500000,4.2,MSFT
UNKNOWN99,250000,2.8,

$ echo $?
1  # PARTIAL — one unresolved row
```

### Pipeline: canon -> rvl (the real workflow)
```bash
# Canonicalize, then compare by canonical ID — no manual join
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon

# With mapping artifacts for audit
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/dec.map.json > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon --json > evidence/rvl.json
pack seal evidence/ --note "Q4 recon with canonical IDs"
```

### Refusals in CSV mode
- If `canon` refuses (exit 2), no CSV is written to stdout
- The refusal JSON object is written to stderr (same envelope as JSON mode)
- `--map-out` file is not created on refusal

### Constraints
- `--emit csv` requires CSV input (not JSONL) — REFUSAL `E_EMIT_FORMAT` otherwise
- If the canonical column name (whether default `<COLUMN>__canon` or explicit `--canon-column`) already exists in the input header — REFUSAL `E_COLUMN_EXISTS`
- The canonical column is always the last column (no column reordering)

---

## Refusal Codes (v0)

| Code | Trigger | Next step |
|------|---------|-----------|
| `E_IO` | Can't read input or registry | Check paths |
| `E_ENCODING` | Unsupported text encoding | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | CSV parse failure | Re-export as standard CSV |
| `E_BAD_REGISTRY` | Registry format invalid (missing `registry.json`, malformed entries) | Fix registry |
| `E_COLUMN_NOT_FOUND` | `--column` doesn't exist in input | Check column name |
| `E_PARSE` | Can't parse JSONL input or unrecognized/missing file extension | Check format; use `.csv`, `.tsv`, `.jsonl`, or `.ndjson` extension |
| `E_EMPTY_INPUT` | Input has no processable data (header only, empty JSONL, or all rows are blank records) | Check input file |
| `E_TOO_LARGE` | Input exceeds `--max-rows` or `--max-bytes` | Increase limits or reduce input |
| `E_EMIT_FORMAT` | `--emit csv` used with JSONL input | Use `--emit json` or provide CSV input |
| `E_COLUMN_EXISTS` | `--emit csv` and canonical column name already exists in input header | Choose a different `--canon-column` name |

### Refusal output contract
Every REFUSAL prints a single JSON object with the shared refusal envelope:
```json
{
  "version": "canon.v0",
  "outcome": "REFUSAL",
  "registry": null,
  "summary": null,
  "mappings": [],
  "unresolved": [],
  "refusal": {
    "code": "E_COLUMN_NOT_FOUND",
    "message": "Column 'cusip' not found in input file",
    "detail": {
      "column": "cusip",
      "available_columns": ["security_id", "isin", "name"]
    },
    "next_command": "canon positions.csv --registry registries/cusip-isin/ --column security_id"
  }
}
```

Refusals are operator handoffs, not dead ends. Every refusal includes either a `next_command` (mechanical recovery) or explicit escalation guidance.

---

## Pipeline Composition

### The core workflow: canonicalize then compare

`--emit csv` is how `canon` plugs into the spine. Canonicalize both sides, then run `rvl` on the canonical files:

```bash
# Monthly loan tape reconciliation with canonical IDs
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon
```

Three commands. No VLOOKUP. No manual join. The canonical column is right there in the file.

### Audit-grade pipeline (with evidence)

```bash
# Canonicalize with mapping artifacts preserved
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/dec.map.json > dec.canon.csv

# Compare on canonical IDs
rvl nov.canon.csv dec.canon.csv --key cusip__canon --json > evidence/rvl.json

# Seal everything as evidence
pack seal evidence/ --note "Nov->Dec recon with canonical CUSIPs"
```

### Inspection and debugging (JSON mode)

```bash
# What resolved and what didn't?
canon tape.csv --registry registries/cusip-isin/ --column security_id \
  | jq '.unresolved[]'

# Resolve reviewed entity aliases (inspect the mapping)
canon aliases.csv --registry registries/entities/ --column entity_label \
  | jq '.summary'

# Canonicalize a JSONL file (stdin via -)
cat events.jsonl | canon - --registry registries/entity/ --column entity_id
```

### Registry maintenance reporting

```bash
# Materialize a registry from a provider-backed seed corpus; this is a live provider example, not an offline fixture
OPENFIGI_API_KEY=xxx canon registry build --source openfigi --seed seeds.csv --seed-column cusip --provider-config exchCode=US --output registries/openfigi-cusip/ --version 2026.03.13

# Prove the OpenFIGI provider path locally without contacting api.openfigi.com
twinning rest --spec ../twinning/tests/fixtures/rest/openfigi_v2_v3/response-stub-schema.yaml --server-variable basePath=v3 --auth-mode shape --run 'canon registry build --source openfigi --seed seeds.csv --seed-column cusip --provider-config id_type=ID_CUSIP --provider-config exchCode=US --provider-config api_key=stub-key --provider-config base_url="$TWIN_BASE_URL/v3/mapping" --output registries/openfigi-cusip/ --version 2026.06.09'

# What changed between two registry versions?
canon registry diff --old registries/openfigi-cusip-v2026.02/ --new registries/openfigi-cusip-v2026.03/

# How well does the current registry cover a seed corpus?
canon registry audit seeds.csv --registry registries/cusip-isin/ --column cusip
canon registry audit seeds.csv --registry registries/cusip-isin/ --column cusip --emit summary

# Establish a self-authored ID convention
canon registry default-id-scheme --registry registries/people/ --prefix PPL --zero-pad 3

# Preview the next local ID without writing
canon registry next-id --registry registries/people/

# Mint one canonical ID with an initial exact alias
canon registry mint --registry registries/people/ --canonical-type person --with-alias 'aliases.json=Jane Doe:MANUAL'

# Add another exact alias to an existing canonical ID
canon registry add-entry --registry registries/people/ --alias-file aliases.json --canonical-id PPL-001 --input 'J. Doe' --rule-id MANUAL
```

---

## Rust Implementation Sketch

### Core crates
- `clap` for CLI (derive)
- `csv` for CSV parsing (streaming, same engine as `rvl`)
- `serde` + `serde_json` for JSON I/O
- `rusqlite` (bundled) for derived registry index
- `blake3` for hashing (witness protocol)

### Registry loading
- Source of truth: directory of JSON mapping files (git-friendly, inspectable, diffable)
- Derived index: SQLite database built automatically on first use, stored inside the registry directory as `_index.sqlite` (`.gitignore`d, rebuilt when stale). Requires write permission to the registry directory on first use or when stale.
- Staleness check: compare `registry.json` version + file modification times against SQLite metadata
- Rebuild is automatic and logged to stderr (no separate subcommand needed for v0)
- This gives git-friendly source files AND sub-millisecond queries against 389K+ entries

### Lookup implementation
- v0: exact-match query (ASCII-trimmed input against registry `input` field) via SQLite
- Alias resolution uses the same exact-match path — the registry author pre-populates all known variants as separate entries
- Multi-column entity resolution does not run inside this lookup path. Domain
  workbenches such as `canon entity` run before lookup promotion, then write flat
  registry entries and sidecars that this lookup path can consume exactly.

### Core data types
```
Mapping { input, canonical_id, canonical_type, rule_id, confidence }
Unresolved { input, reason }
Registry { id, version, source, entries }
```

### Processing

**JSON mode (`--emit json`, default):**
- Parse input CSV/JSONL in streaming fashion, collecting unique input values into a `HashSet`
- Resolve each unique value once against the SQLite index
- Build `mappings` and `unresolved` lists in memory (bounded by unique value count, not row count)
- Emit single JSON object at end

**CSV mode (`--emit csv`):**
- Pass 1: stream input, collect unique column values into a `HashMap<input_value, canonical_id|None>`
- Resolve all unique values against the SQLite index (single batch query)
- Pass 2: stream input again, appending the canonical column to each row using the lookup map
- Memory: bounded by unique value count (the lookup map), not row count
- The CSV writer preserves the input's delimiter and quoting
- If `--map-out` is specified, the JSON mapping artifact is written after pass 2 completes

### Witness protocol
- Same pattern as `rvl`: hash inputs, hash output, append witness record
- `output_hash`: in JSON mode, hash the JSON mapping blob. In CSV mode, hash the CSV bytes as they stream through stdout (incremental BLAKE3 — the hasher sees the same bytes the pipe does).
- ~100-150 LOC in a `witness` module
- Never block on witness failure

---

## Testing Philosophy

Must-pass (v0)
- exact match resolves correctly with registry version recorded
- missing column in input => REFUSAL (`E_COLUMN_NOT_FOUND`)
- malformed registry => REFUSAL (`E_BAD_REGISTRY`)
- empty registry (valid format, zero entries) => all inputs unresolved, outcome UNRESOLVED, exit 1
- all inputs resolve => outcome RESOLVED, exit 0
- some inputs unresolved => outcome PARTIAL, exit 1
- zero inputs unresolved, all resolved => outcome RESOLVED, exit 0
- zero inputs resolved, all unresolved => outcome UNRESOLVED (not PARTIAL), exit 1
- input with blank records => blanks skipped, counts correct
- JSONL input with missing field => unresolved with `"missing_field"` reason
- JSONL input with `null` field value => unresolved with `"null_value"` reason (not string-coerced)
- CSV input with empty column value (non-blank row) => unresolved with `"empty_value"` reason
- JSONL input with object/array field value => unresolved with `"non_scalar_value"` reason
- file with no extension or unrecognized extension => REFUSAL (`E_PARSE`)
- registry version is recorded in output (reproducibility)
- same input + same registry version + same `--registry` path = byte-identical output (determinism). Note: `registry.source` in JSON output reflects the CLI argument verbatim, so different paths to the same registry produce different `registry.source` values — all other fields are path-independent.
- large input (100k+ rows) completes without OOM
- `--max-rows` / `--max-bytes` enforcement => REFUSAL (`E_TOO_LARGE`)
- identifier encoding: non-UTF-8 bytes in input values => `hex:` rendering
- alias resolution: variant names resolve to same canonical ID
- Unicode edge cases in input values handled without panic
- `--emit csv`: output has original columns + canonical column appended
- `--emit csv`: resolved rows have canonical ID, unresolved rows have empty canonical column
- `--emit csv`: delimiter and quoting match input file
- `--emit csv`: default canonical column name is `<column>__canon`
- `--emit csv`: `--canon-column` overrides the name
- `--emit csv` + JSONL input => REFUSAL (`E_EMIT_FORMAT`)
- `--emit csv` + canonical column name already in header => REFUSAL (`E_COLUMN_EXISTS`)
- `--emit csv` + `--map-out`: JSON mapping artifact matches what `--emit json` would produce
- `--emit csv`: PARTIAL exit code 1 but CSV is still fully written (unresolved rows are visible, not dropped)
- `--emit csv` + `--emit json` consistency: for the same input + registry, the CSV canonical column values correspond to the `mappings[].canonical_id` values in JSON output after stripping the identifier encoding prefix (CSV has `AAPL`, JSON has `u8:AAPL` — same value, different representation)
- `registry diff`: deterministic added/removed/changed/unchanged counts and detail for known registry fixtures
- `registry audit`: exit 0 even when unresolved seeds exist, with stable `resolved`, `unresolved`, `canonical_targets`, and `rule_hits` sections
- `entity review export`: same result artifact bytes produce byte-identical review artifacts with stable review IDs
- `entity review import`: clean reviewed decisions write alias, trusted-anchor, pending-escrow, and cannot-link patches with proof hashes
- `entity review import`: refuses duplicate review IDs, malformed decisions, stale registry snapshots, trusted-anchor conflicts, and unaudited alias/anchor promotions
- `entity review` CSV export/import round-trips decisions and registry snapshot metadata

Never allow
- silent resolution failures (every unresolved entry must be reported)
- auto-accepting probabilistic matches as deterministic
- resolving against an unversioned registry
- different output for the same input + registry version

---

## Success Criteria (Real World)
- `canon --emit csv | rvl` replaces a VLOOKUP-then-eyeball workflow in under 60 seconds
- an analyst opens the `.canon.csv` in Excel and sees the canonical column right there — no join required
- registry diffs show exactly what changed between versions
- registry audits show which seed values resolved, which stayed unresolved, and which canonical targets and rule IDs were exercised
- the mapping output is inspectable: every resolution traceable to a registry entry + rule ID
- "who is Wells Fargo in this dataset?" has one answer, with a rule ID
- someone deletes a VLOOKUP spreadsheet because `canon --emit csv` made it unnecessary

If any feature makes the mapping less inspectable or the pipeline less composable, cut it.

---

## v1 Ideas (Only If v0 Is Loved)

### `canon suggest` — probabilistic matching
- LLM-assisted or fuzzy-matching mode for unresolved entries
- Emits suggestions with `"confidence": "suggested"` — never auto-accepted
- Human reviews suggestions, accepts into registry, re-runs for deterministic resolution
- This is the onramp for new registries: run `canon suggest`, curate results, freeze into a registry

### Registry `match_mode` (normalized matching)
- Per-registry `match_mode` field in `registry.json` (e.g., `"exact"`, `"case_insensitive"`, `"normalized"`)
- Eliminates the need for registries to enumerate all case variants
- Normalization rules defined per mode, documented and deterministic
- `strsim` crate (Jaro-Winkler, Sorensen-Dice) for fuzzy candidate scoring in suggest mode

### Future workbench directions
- Cross-source structural linkage lives under `canon entity link`; future work should extend that workbench namespace rather than reintroduce a sibling public command
- Property identity may use multi-column matching (address + name + coordinates -> canonical ID)
- Geospatial matching via `geo` + `rstar` (Haversine + R-tree) may be useful inside a property workbench
- Phonetic blocking via `rphonetic` (Metaphone, Soundex) may be useful for candidate generation, not auto-accepted lookup
- H3 hex blocking via `h3o` may be useful for property matching at scale (389K+ entries)
- Address normalization (US abbreviations: ST->STREET, AVE->AVENUE, etc.) belongs in the workbench strategy, not the core lookup kernel

### ID validation
- CUSIP format validation + check digit computation via `cusip` crate
- ISIN format validation via `isin` crate
- Validate input IDs before lookup (refuse malformed IDs with a new `E_INVALID_ID` code)

### Fellegi-Sunter probabilistic matching (v1+)
- Comparison vectors -> log-likelihood ratios -> threshold
- Weights trained offline, applied deterministically at runtime
- ~500 LOC of Rust

### `libpostal` integration (v1+)
- Statistical address parser for complex address variants
- FFI binding (~2 GB data files)
- Only for entity resolution registries with address matching

### Registry push/pull
- `canon push` / `canon pull` for data-fabric integration
- Share registries across teams with version tracking

### Decision notes
**Entity registry format:** The v0 registry format (flat JSON mapping files)
handles ID mapping and alias resolution cleanly. Richer identity domains may
need sidecars for anchors, escrow, review, and proof metadata. The durable
lookup asset is still flat registry entries plus derived indexes; multi-column
matching logic belongs in workbench execution, not in the core lookup
implementation.

---

Final rule: If you can't explain the mapping to someone staring at a spreadsheet, it doesn't ship.
