# canon entity — agent guide

How an agent (or operator) takes a CSV of names to a reviewed registry
proposal with `canon entity`, without reading
[`PLAN_ENTITY_WORKBENCH.md`](PLAN_ENTITY_WORKBENCH.md). The plan is the
specification; this is the how-to. If they disagree, the plan wins.

Boundary reminder ([`IDENTITY_ARCHITECTURE.md`](IDENTITY_ARCHITECTURE.md)):
the workbench produces *proposals*. Runtime lookup stays exact-match against a
reviewed, versioned registry. Nothing below loosens that.

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
| Audit | `canon entity audit wd/run/run.json --suite SUITE` | audit artifact |
| Promote / apply | `canon entity promote …` / `canon entity apply …` | registry knowledge / canonicalized rows |

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
| support | token_overlap | declared only |
| support | string_similarity | yes |
| support | tfidf_cosine | yes |
| support | date_transposed_digits | yes |
| support | two_token_reversal | yes |
| support | alias_patch_match | declared only |
| support | reviewed_alias | declared only |
| cannot_link | alias_patch_distinct | declared only |
| cannot_link | anchor_conflict | yes |
| cannot_link | attribute_conflict | yes |
| cannot_link | protected_token_conflict | yes |
| cannot_link | related_distinct_phrase | declared only |
| cannot_link | conflicting_anchor | declared only |
| cannot_link | same_property_distinct_rank | declared only |
| cannot_link | role_conflict | declared only |
| cannot_link | protected_anchor_conflict | declared only |
| cannot_link | segment_conflict | declared only |
| cannot_link | context_conflict | declared only |
| cannot_link | platform_label_guard | declared only |
| cannot_link | division_boundary | declared only |
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
(`role_conflict`, `platform_label_guard`, `division_boundary`, `division_of`,
`parent_subsidiary_context`) are also declared only today.

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
- `score_units` — `0` disables the operator.

A pair is cut (hard cannot-link) only when its protected-token sets differ
**and** the two names still share an unprotected token. "Acme Coffee" vs
"Acme Coffee Shop" is not cut; unrelated names are not made distinct by a
stray digit.

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
