# canon entity — agent guide

How an agent (or operator) takes a CSV of names to a reviewed registry
proposal with `canon entity`, without reading
[`PLAN_ENTITY_WORKBENCH.md`](PLAN_ENTITY_WORKBENCH.md). The plan is the
specification; this is the how-to. If they disagree, the plan wins.

Boundary reminder ([`IDENTITY_ARCHITECTURE.md`](IDENTITY_ARCHITECTURE.md)):
the workbench produces *proposals*. Runtime lookup stays exact-match against a
reviewed, versioned registry. Nothing below loosens that.

## Start an evidence session

For a new registry, initialize the session and registry together:

```bash
canon entity session init \
  --goal "dedupe BDC borrowers" \
  --registry registries/organisations \
  --work-dir work/bdc-session \
  --registry-id organisations \
  --prefix ORG \
  --zero-pad 6
```

This creates `registry.json`, an empty `aliases.json`, and an append-only
session manifest under `work/bdc-session/.canon/entity/publications/v1`.
`canon registry lint registries/organisations` and
`canon registry next-id --registry registries/organisations` are useful
follow-up checks; an empty registry correctly leaves exact lookup unresolved.
Repeat the same command to get an idempotent `already_initialised` result.
Changing the goal or registry id for an existing work directory is refused;
use a new `--work-dir`. If the registry already exists, `--registry-id`
and `--prefix` are only needed when you want to check the requested registry id.
The next session steps are `canon entity session ingest` for observations and
`canon entity session assert` for typed evidence.

## Ingest observations

Append one observation JSON object per line to the session. The file may be
replaced with `-` to read JSONL from stdin:

```bash
canon entity session ingest work/bdc-session observations.jsonl --emit json
cat observations.jsonl | canon entity session ingest work/bdc-session - --emit json
```

Each row requires `obs_id`, `entity_type`, `surface`, and
`source.channel`. `keys`, `attrs`, and `context` are optional objects whose
values must be scalar or null. Accepted rows are stored in deterministic,
content-addressed observation shards. A malformed row is reported with its
line, rejection code, and `fix`, while other rows are still appended; exit 1
means at least one row was rejected. Repeat an exact ingest to receive
`accepted=0` and an `already_present` count without creating a new generation.
Ingest trims only ASCII whitespace around required text fields; a surface such
as `` `tick` `` is retained as-is.

## Assert evidence

Append typed evidence assertions to the same session, from a JSONL file or
stdin:

```bash
canon entity session assert work/bdc-session evidence.jsonl --emit json
cat evidence.jsonl | canon entity session assert work/bdc-session - --emit json
```

Short assertion rows compile into the existing `canon.evidence.v1` record:

```json
{"type":"name_equivalent","left":"obs-a","right":"obs-b",
 "basis":{"rule":"casefold+strip_punct","left_value":"A, LLC","right_value":"A LLC","steps":["casefold","strip_punctuation"]},
 "source":{"channel":"script","id":"goldset:1","locator":"row:4","content_hash":"blake3:<64 hex>"},
 "asserted_by":"script:goldset"}
```

The supported types are `same_key`, `same_slot`, `attribute_agreement`,
`name_equivalent`, and `asserted_alias` (positive pair support), `distinct`
(a hard cannot-link requiring `basis.authority`), and `related` (a contextual
pair relation requiring `basis.relation` and `basis.subject`). `asserted_alias`
requires a locator and content hash for non-human asserters. Complete
`canon.evidence.v1` records are accepted as well. Rows are canonicalized and
stored in the `entity-session-evidence` publication stream; malformed rows are
reported individually with line, code, message, and fix while valid rows are
still appended. Exit 1 means at least one row was rejected.

## Solve a session

Use a declared rule table to classify exact observation nodes as tier A, tier B,
review, standalone, or contradiction:

```bash
canon entity session solve work/bdc-session --rules session-rules.yaml --emit json
```

The solve reads the committed observation and evidence generations, performs
exact registry lookup for incumbents, and publishes `solve/solve.json` in the
`entity-session-solve` stream. A shared authoritative key or asserted alias is
tier A. Tier B needs the declared number of distinct eligible evidence types
and independent source lineages; missing corroboration stays review. `distinct`
is a contradiction, `related` is reported as context and never joins a
cluster, and a node with no positive evidence remains standalone. Solve exit 0
means the deterministic artifact completed even when it reports review or
contradiction outcomes.

The minimum rule table shape is:

```yaml
version: canon.entity.session.rules.v1
keys:
  authoritative: [cik, lei]
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
```

## The loop

```text
rows.csv -> [profile] -> entity run -> review export -> human decision
         -> review import -> audit -> promote -> apply
```

| Step | Command | Output |
|------|---------|--------|
| Pick a profile | `canon entity profile list` / `profile init <ID> --output p.yaml` | a profile YAML to edit |
| Run | `canon entity run rows.csv --profile p.yaml --strategy p.yaml --registry REG --work-dir wd --emit summary` | `wd/run/run.json`, `wd/solve/solve.json`, … |
| Export the queue | `canon entity review export wd/run/run.json --include all --emit json` (or `--emit csv`) | `canon_entity_review.v1` queue |
| Decide | fill each item's `decision`; items left blank are not imported | decided queue |
| Import | `canon entity review import queue.json --registry REG --next-version X.Y.Z` | new registry version |
| Audit | `canon entity audit wd/run/run.json --suite SUITE --emit json > audit.json` | audit artifact (see "Audit suites and gold labels") |
| Promote | `canon entity promote wd/run/run.json --audit audit.json --registry REG --next-version X.Y.Z` | registry knowledge for the audited run; refuses an audit that does not certify this exact run |
| Apply | `canon entity apply wd/run/run.json --rows rows.csv --registry REG --column COL --out out.csv [--allow-partial-output]` | rows with canonical IDs appended; refuses before writing if any row stays unresolved unless `--allow-partial-output` (then exit 1) |

Notes from running this loop:

- `review export` accepts the run (`run.json`) or its solve (`solve.json`).
  Exporting the run reads the items from the solve the run hash-binds and keeps
  the queue bound to the run, which is the artifact `audit` and `promote`
  certify. A solve that does not match the run's recorded hash is refused.
- `--artifact native-review` accepts `canon_entity_link.v1`, `canon_entity_run.v1`,
  `canon_entity_solve.v1`, and legacy v0 run/solve artifacts. A v1 run export
  follows its single hash-bound solve stage and binds the native review to the
  run hash; the exact solve version and hash stay in the review's upstream
  references. A direct solve export binds to that solve hash. The native review
  envelope remains `canon_entity_native_review.v0`.
- Import native decisions with `canon entity review import decisions.json
  --registry REG --next-version X.Y.Z --source-review review.json`. Cluster
  assignment requires an explicit `target_canonical_id`; import returns proposal
  patches and does not write registry truth.
- Alias proposals take `accept_alias` or `reject_alias` as their `decision`.
- A component whose positive-support edges contain a hard cannot-link is
  reported as state `contradiction` (reason `hard_cannot_link_constraint`),
  never merged and never promotable. By default the solver does not split it.
  To resolve numbered or lettered siblings deterministically, opt in with
  `solve: {hard_cannot_link: split_direct}` (see "Solver options" below).

## What the agent decides vs. what a human must

- Agent: extract observations, choose and author the profile, run the
  workbench, read evidence and explain output, prepare the decided queue for
  review.
- Human: new canonical IDs default to human review. Only an explicit
  `promotion.new_ids: auto_accept` strategy, backed by a passing frozen labeled
  audit, may stand in for that. No policy may accept a component containing a
  hard cannot-link.
- Nobody: matching at lookup time. It stays exact.

## Audit suites and gold labels

There are two different things called "suite"; know which one you need.

**1. The suite directory given to `entity audit --suite DIR`.** For a
`canon_entity_run.v1` / `canon_entity_solve.v1` artifact the directory may hold
a `manifest.json`:

```json
{"id": "my-suite", "version": "v1",
 "gates": [{"gate_id": "G01", "label": "artifact continuity", "passed": true,
            "expected": "v1_self_hash_valid", "actual": "v1_self_hash_valid"}]}
```

`gate_id` must be one of `G01`..`G15`, unique, with a non-empty label; a gate
with `"passed": false` makes the audit refuse with `E_ENTITY_AUDIT_GATE`. A
directory with no `manifest.json` (or an empty `gates` list) gets the two
default gates `G01` artifact continuity and `G14` promotion preflight. **These
`passed` flags are asserted by whoever wrote the manifest; canon does not
compute them.** Treat such an audit as proof that the artifact chain is intact
(the audit certifies the exact run hash that `promote` then checks), not as
evidence that the matches are correct. Correctness evidence comes from gold
labels, below.

**2. The frozen policy suite, which *is* computed.** It exists only when the
strategy declares `promotion: {new_ids: auto_accept, auto_accept: {...}}`.
`audit.suite` in that policy names a directory holding `policy_suite.json`,
and the audit re-runs the same strategy on that corpus and scores what the
policy would accept against your labels. Nothing in the file can claim a
precision; every input is pinned by `blake3:` hash and a mismatch refuses
(`suite_input_hash_mismatch`).

```text
SUITE/
  policy_suite.json   rows, rows_hash, profile, profile_hash, registry,
                      registry_snapshot_hash, gold, gold_hash,
                      gold_provenance, work_dir   (unknown keys are refused)
  gold.json           JSON array: [{"surface_id": "...", "entity_id": "..."}, ...]
```

- `gold.json` labels *surfaces with an entity*, not pairs: surfaces that share
  an `entity_id` are the same entity, different ids are distinct. It must cover
  exactly the prepared surface universe of the suite corpus (get the ids from
  `wd/prepare/surfaces.jsonl` after one run of the same rows and profile), have
  at least 3 labels and 2 entities, and at least one entity with more than one
  surface (so both positive and negative pairs exist).
- `gold_provenance` must be non-empty and states who labeled it. Operator
  labels are not replaceable by agent or model labels for a real-data claim.
- Hashes are `canon::witness::hash_bytes`, i.e. `blake3:` + hex of the file
  bytes (`b3sum` hex with that prefix). Any edit to rows, profile, registry
  snapshot or gold needs a new manifest.
- The policy passes only if the computed pair and component precision reach
  `min_pair_precision` / `min_component_precision`. A fixture-built suite
  (`tests/entity_profile_instrument.rs`,
  `declared_policy_runs_a_frozen_audit_then_promotes_exact_aliases`) proves the
  mechanics, not quality; label real samples before claiming a precision.

### Gold JSONL for `entity calibrate sweep`

`canon entity calibrate sweep RESULT --gold gold.jsonl --strategy strat.yaml`
reads one JSON object per line (a JSON array, or an object with a `gold`,
`labels`, `pairs` or `records` array, also works). Pair labels, not entity
labels:

```jsonl
{"left_surface_id": "s_aaa", "right_surface_id": "s_bbb", "label": "same_entity"}
{"left_surface_id": "s_aaa", "right_surface_id": "s_ccc", "label": "distinct_entity", "critical": true}
```

- Accepted labels: `same`/`same_entity`/`match`/`must_link`/`positive` and
  `distinct`/`distinct_entity`/`non_match`/`cannot_link`/`negative`.
  Anything else is refused.
- `critical: true` marks a false merge that must never happen; any accepted
  critical false merge fails the tuple.
- The sweep re-scores the run's own evidence pairs at candidate thresholds and
  recommends one only if a tuple meets `canon.entity.quality.v1`: precision
  at least 99.50% **and** recall at least 98.00% with zero critical false
  merges. Otherwise the status is `blocked` (`no threshold tuple satisfied
  canon.entity.quality.v1`) and no threshold is suggested.
- Deriving pair gold from entity gold: for every unordered pair of labeled
  surfaces, emit `same_entity` if their `entity_id` is equal, else
  `distinct_entity`. Label the sibling pairs that matter (RR vs RR H, Fund II
  vs Fund IV) explicitly as `distinct_entity`; they are the cases a score
  alone cannot separate.
- What a profile needs to pass: score-only support cannot separate suffix
  siblings from spelling variants, so a profile that reaches the gate needs
  anti-merge evidence for them: `protected_token_conflict` for numbered
  siblings, `attribute_conflict` / `anchor_conflict` where rows carry an
  attribute or registry-grade anchor. Check the operator catalog: only
  operators marked "yes" affect the sweep.

Quickest honest loop for a new profile: run once, copy surface ids from
`wd/prepare/surfaces.jsonl`, write `gold.json` (policy suite) or the pair
JSONL (sweep), run `calibrate sweep`, add anti-merge operators until it stops
reporting `blocked`, then pin the suite and run `entity audit`.

## Solver options (`solve:` in the strategy file)

Both settings are opt-in; without a `solve:` section the solver behaves exactly
as described above. They live in the file passed as `--strategy` (not in the
profile).

```yaml
strategy_id: my_strategy
strategy_version: 0.1.0
solve:
  hard_cannot_link: split_direct   # default: contradiction
  standalone_surfaces: include     # default: omit
```

**`hard_cannot_link: split_direct`.** Support edges whose two endpoints are
*directly* hard-cannot-linked are cut, and components are recomputed. "Sub III"
and "Sub IV" stop sharing a component; spelling variants of one vehicle
("Sub III LLC" / "Sub III, L.L.C.") stay merged because no hard cannot-link sits
between them. Every surface that lost an edge is kept as its own component, so
nothing disappears. A conflict that survives the cut (A~B and B~C supported,
A and C cannot-linked, no direct A~C support edge) stays a `contradiction` for a
human. The cut edges stay visible in `evidence/evidence.jsonl` (both the support
and the cannot-link hits), and `summary.counts.diagnostics_split_direct_cut_edges`
records how many were cut. Hyperedges from exact buckets are never cut.

**`standalone_surfaces: include`.** By default a surface with no support edge
and no incumbent ID is absent from the solve: it is not an entity and not a
review item. With `include` it becomes a one-surface component in state
`escrow`, reason `standalone_surface_no_support`, and is counted in
`summary.counts.diagnostics_standalone_surfaces`.

## Minting IDs for entities that matched nothing

`entity run` never mints an ID for a lone surface, and that is by design: the
only new-ID route is `promotion.new_ids: auto_accept`, which refuses any policy
that allows a one-surface component or zero positive support, and requires a
passing frozen audit. An ID for an entity with no matching evidence is an
operator-accepted fact, so it goes through the registry maintenance commands:

1. Run with `standalone_surfaces: include` (and `split_direct` if you have
   sibling vehicles) to list every surface that still needs an ID.
2. Review the list, then write one JSON object per line:
   `{"with_alias": ["aliases.json=Acme Fund II, LLC:MANUAL"], "canonical_type": "vehicle"}`
   (`with_alias` is the same `FILE=INPUT:RULE_ID` spec as `registry mint`;
   `canonical_type` is optional per line).
3. `canon registry mint-batch --registry REG --canonical-type vehicle --from batch.jsonl`

`mint-batch` allocates sequential IDs from the registry's `default_id_scheme`
(or `--prefix`), writes every alias, and bumps the version **once**. It is
atomic: any refusal (a duplicate input inside the batch or already in the
registry, a bad alias file, a lint error) leaves the registry byte-identical.
`--emit plain` prints one new ID per line.

## Recording lookup misses for the next cut

Plain exact lookup can record what it did not find, so a driver can turn misses
into its next bounded batch. Canon records; it does not schedule or fetch.

```bash
canon rows.csv --registry REG --column name \
  --inbox-out misses.inbox.json --inbox-project P --inbox-run R1 \
  --inbox-seen-at 2026-11-16T00:00:00Z [--inbox-raw-ref]
```

Each unresolved value becomes one `canon.unresolved.inbox.v1` item (one item per
event key; a value missed again in a later run, with another `--inbox-run`,
gains an occurrence). The file is created or merged and replaced atomically;
stdout gains an `inbox{path,items_added,items_total,artifact_content_hash}`
block and nothing else changes (stderr in `--emit csv` mode). Canon reads no
clock, so `--inbox-seen-at` is required. The missed string is never stored:
`--inbox-raw-ref` adds an external reference to the input file, column and input
hash, and the driver reads the string from there. List what was recorded with
`canon inbox list --inbox misses.inbox.json`.

## Authoring a profile

Start from `canon entity profile init <built-in> --output p.yaml`. A profile
must declare non-empty `normalized_views`, `prepare`, `patch_namespaces`, and
all three evidence lanes (`support`, `cannot_link`, `relation_hints`); a
missing field is refused with `E_ENTITY_PROFILE` listing `missing` plus the
`supported_operators` per lane.

**Who owns what.** `entity run` takes its evidence lanes from the *profile*.
`--strategy` is read for its id, version, budgets and content hash; an
`evidence:` block in a separate strategy file has no effect. (The shipped
fixtures pass the profile file as the strategy, which is why that works there.)

### Candidate retrieval budgets

Candidate budgets belong to the strategy file under `block:`, not to the
profile's `blocking:` retrieval section:

```yaml
block:
  max_candidates_per_surface: 100
  max_candidates_per_operator: 25000
  max_candidates_per_run: 25000
  max_exact_bucket_size: 10000
```

The default per-operator and per-run limits scale from the per-surface limit
and the prepared surface count. An explicit value remains authoritative, but
the absolute safety ceiling still applies. If a run crosses a limit before
writing candidates, the refusal names the exact key (for example,
`block.max_candidates_per_run`), says that it belongs in the strategy file,
and includes a ready-to-paste `block:` fragment. A budget under `blocking:` or
in the profile file is reported as misplaced rather than ignored.

### Operator catalog

Every operator the profile validator accepts, and whether `canon entity run`
executes it. **"declared only" means the validator accepts it but `run` does
nothing with it** — `entity run --emit summary` prints a warning naming them.
Relation hints are never executed per operator: `run` emits one generic review
hint per retained candidate.

<!-- operator-catalog:start -->
| Lane | Operator | Executed by `entity run` |
|------|----------|--------------------------|
| support | exact_view | yes |
| support | anchor_match | yes |
| support | isin_cusip_arithmetic | yes |
| support | token_overlap | yes |
| support | string_similarity | yes |
| support | tfidf_cosine | yes |
| support | date_transposed_digits | yes |
| support | two_token_reversal | yes |
| cannot_link | anchor_conflict | yes |
| cannot_link | attribute_conflict | yes |
| cannot_link | protected_token_conflict | yes |
| relation_hints | related_brand_family | declared only |
| relation_hints | possible_successor_predecessor | declared only |
| relation_hints | same_parent_or_sponsor | declared only |
| relation_hints | division_of | declared only |
| relation_hints | parent_subsidiary_context | declared only |
| relation_hints | cross_profile_alignment | declared only |
| relation_hints | context_alignment | declared only |
| relation_hints | segment_alignment | declared only |
| relation_hints | issued_by | declared only |
| relation_hints | superseded_by | declared only |
<!-- operator-catalog:end -->

`tests/entity/run_review_handoff.rs` fails if this table and the code disagree.
Generic operators have a `view:` and `params:`; the Reg-AB-flavored ones
(`division_of` and `parent_subsidiary_context`) remain relation-hint
declarations; `entity run` emits one generic review hint for retained
candidates rather than executing each relation operator separately.

A support operator with no threshold (`string_similarity`/`tfidf_cosine`
without `min_score_units`) is disabled by design and produces no evidence.

### `protected_token_conflict`

Keeps numbered siblings apart ("Fund II" vs "Fund IV", "Sub III" vs "Sub IV").
Required: `view`. Optional params:

- `protect` — comma list of `digits`, `roman`, `none`. Default `digits,roman`.
  `digits` protects any token containing a digit. `roman` protects series
  numerals written with only i/v/x, two letters or longer, up to xxxix.
  Single letters and other Roman letters are not protected (initials and words
  like "mix" are common); list them in `protected_tokens` if you need them.
- `protected_tokens` — extra comma-separated tokens to protect.
- `shared_context_view` — optional declared normalized view. Exact, non-empty
  equality on both surfaces can scope a protected-token conflict even when
  their unprotected whitespace tokens are disjoint. Missing, blank or different
  values supply no such scope; an unknown view refuses. This is conflict
  evidence only, never identity support or acceptance authority.
- `score_units` — `0` disables the operator.

By default, a pair is cut (hard cannot-link) only when its protected-token sets differ
**and** the two names still share an unprotected token. "Acme Coffee" vs
"Acme Coffee Shop" is not cut; unrelated names are not made distinct by a
stray digit.

With `shared_context_view`, a differing protected-token set can also cut the
pair within that explicitly shared context. Tokenization is unchanged: for
example, `Example 2 Company` and `Example2Company` have different whole protected
tokens and are conservatively cut when they share that context, despite the
same numeral. This option does not establish numbered-name recall or split
condensed identifiers into inferred words.

## Enrichment: where agent-sourced facts live

Decision (bd-6znd): enrichment stays **upstream** of the workbench. The
workbench has no "agent sources facts" step and no enrichment-row contract.

- Add the facts as extra columns on the rows, with sibling provenance columns
  (`<fact>_source`, `<fact>_retrieved_at`, `<fact>_receipt`).
- List the provenance columns in the profile's `prepare.provenance_fields` so
  they ride along on the prepared surface for review.
- Map an agent-sourced fact to `context_fields` (support, never a hard cut) by
  default. Map it to `anchor_fields` only when its source is registry-grade;
  an anchor conflict is a hard cannot-link.

Revisit only if a second use case needs a shared enrichment shape.
