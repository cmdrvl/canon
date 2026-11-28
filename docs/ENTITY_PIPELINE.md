# The profile-and-strategy entity pipeline

This is the older way to resolve entities with canon: `entity run` and its
stages, driven by a profile and a YAML strategy, with audit, review and promote.
New work should start with an evidence session (see the [README](../README.md)
and the [agent guide](ENTITY_WORKBENCH_AGENT_GUIDE.md)). These commands remain in
the binary and are still tested. `canon entity link` needs the `workbench` build
feature, which released binaries include.

## Cross-Source Linkage (`canon entity link`)

`canon entity link` is for the moment before a cross-reference registry exists.
Give it reference rows, target rows, and a YAML strategy that says how fields
correspond. It runs under the entity workbench namespace, preserves
reference/target directionality, filters candidate pairs, scores deterministic
assertions, and emits `canon_entity_link.v1` with
`canon_entity_link_decisions.v1` and a hash-bound
`canon_entity_link_observation_surface_bindings.v1` sidecar.

This is still not the core lookup path. The normal
`canon <INPUT> --registry ...` command does exact lookup only. Link mode is a
workbench for manufacturing audited cross-reference entries that normal lookup
can use later. There is no public compatibility alias for the superseded
standalone resolve namespace.

Released binaries include `canon entity link`. A source build needs
`--features workbench`.

`--profile` and `--work-dir` stay bracketed in generated-help syntax because
the parser emits a structured `E_ENTITY_INPUT_CONTRACT` refusal when either is
omitted. Successful link execution requires both flags, and omission performs
no writes.

Run the committed fixture corpus as JSON. The `--work-dir` path should be a
fresh temporary directory owned by the caller:

```bash
canon entity link \
  tests/fixtures/resolve/tapes/reference_loans.csv \
  tests/fixtures/resolve/tapes/target_loans.csv \
  --profile cmbs_tenant_label \
  --strategy tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml \
  --registry tests/fixtures/registries/resolve-servicers \
  --work-dir /tmp/canon-entity-link-work \
  --gold tests/fixtures/resolve/gold/loan_matches.jsonl \
  --no-witness
```

Summary mode is compact for operators:

```bash
canon entity link \
  tests/fixtures/resolve/tapes/reference_loans.csv \
  tests/fixtures/resolve/tapes/target_loans.csv \
  --profile cmbs_tenant_label \
  --strategy tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml \
  --registry tests/fixtures/registries/resolve-servicers \
  --work-dir /tmp/canon-entity-link-summary \
  --emit summary \
  --no-witness
```

`--write-back` is accepted by the parser as an explicit handoff request, but
the current public v1 path refuses before work-dir or registry mutation:

```bash
canon entity link <REFERENCE.csv> <TARGET.csv> \
  --profile entity_profile \
  --strategy <STRATEGY.yaml> \
  --registry <REGISTRY_DIR> \
  --work-dir /tmp/canon-entity-link-write-back \
  --suite <SUITE_DIR> \
  --gold <GOLD.jsonl> \
  --write-back
```

Use the emitted artifact with `canon entity review export`, review/import,
audit, promote, and apply to add accepted knowledge through the transactional,
versioned registry path. Link artifacts never authorize direct mutation by
themselves.

Current link-mode limits: exactly two row sets, deterministic local operators,
no network or model runtime, and no direct registry write-back. Registry
knowledge changes go through review, audit, promotion, and exact replay.

---

## Entity Workbench (`canon entity`)

The same entity appears as "Wells Fargo & Company" in one document, "Wells Fargo Bank, N.A." in another, and "WFB" in a third. Three names, one issuer. `canon entity` compiles profiled observations into reviewed registry knowledge through deterministic artifacts — no ML model dependency, no live provider lookup, no open-ended runtime guessing.

The pipeline is YAML-driven: a **strategy file** defines which fields to observe, how to normalize names, which blocking operators generate candidates, how to score evidence, and what thresholds the solver uses to merge, link, review, or abstain. Same strategy + same input + same registry = same artifacts, every time.

`canon entity` is a resolution workbench, not the core lookup path. It manufactures registry knowledge through evidence, audit, review, and promotion. After promotion, ordinary `canon` runs still resolve the resulting aliases through exact lookup.

Human review is the default. Cluster strategies may explicitly declare a
`promotion.new_ids: auto_accept` policy with evidence requirements, a component
size limit, and a frozen labeled audit suite. `entity audit` runs that strategy
on pinned inputs and measures pair and component precision; `entity promote`
requires the matching passing audit and policy decision ledger. Hard cannot-links
never qualify, and subsequent human distinct/relation decisions override policy
aliases. See the [policy contract](PLAN_ENTITY_WORKBENCH.md#declared-promotion-authority)
for configuration and suite format. No automatic instrument policy or live truth
certification is enabled by the built-in profile.

There are two public entity modes:

- **Cluster mode:** `canon entity run` groups observations inside one profiled corpus and emits solved clusters, escrow, review, and promotion artifacts.
- **Link mode:** `canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` aligns two row sets through the same typed request and artifact path used by project mode. It is for cross-source linkage, not a hidden alias for edge scoring.

`canon entity calibrate sweep <RESULT|EVIDENCE> --gold <GOLD.jsonl> --strategy
<STRATEGY.yaml>` is a read-only threshold-selection report compiler for entity
workbench score artifacts. It builds a deterministic integer grid from observed
labeled score units, emits one truth-space row per threshold tuple with rates in
integer basis points, and recommends the tuple that maximizes auto-accept while
satisfying frozen `canon.entity.quality.v1` gates: precision `>= 9950` bps,
recall `>= 9800` bps, and critical false merges `== 0`. If no tuple passes,
`recommendation.status` is `blocked` and no fallback tuple is selected. The
report includes a proposed YAML threshold fragment for manual application only;
the command never writes registry files, work directories, witness ledgers, or
the strategy file.

`canon entity calibrate em <EVIDENCE.jsonl> --strategy <STRATEGY.yaml>` is a
read-only support-weight suggestion report compiler for entity evidence
artifacts. It collapses per-pair support-lane hits into distinct agreement
patterns before fixed-point EM, reports m/u probabilities and log-odds-derived
score units, and emits a proposed `support_scores` YAML fragment for manual
review only. Constant fields, zero-count cells, and non-converged runs produce
typed warnings and no proposed weight for the affected operator.

The native entity workbench cache contract records `canon_entity_index_cache_receipt.v0`
stage artifacts. Public `canon entity run` and `canon entity link` expose
`--cache-mode enabled|disabled`, defaulting to `enabled`. A genuine warm cache
hit requires an enabled, reusable receipt and is reported as `cache_enabled` with
status `hit`; disabled cache mode bypasses reuse and records a non-reusable
`cache_disabled` receipt with status `bypassed`. The receipt bundle hash covers
the complete index bundle bytes: index artifact, cache key, postings, and
diagnostics. Run/link artifacts bind the cache receipt stage to the native index
stage, refuse stale or tampered bundles before reuse, and preserve semantic
outputs across enabled and disabled cache modes.

Link observation IDs and prepared `surface_id` values are separate namespaces.
The link artifact therefore carries a hash-bound observation/surface sidecar.

`canon entity review export` defaults to the existing queue review contract:
`--artifact queue --emit json|csv` keeps the v1/legacy review queue behavior and
does not change the default review path. Explicit
`--artifact native-review --group-by signature --emit json|csv|html` emits
`canon_entity_native_review.v0` for native solve, run, or link artifacts. The
native artifact carries a self-hash, binds run/policy/registry context, and
validates exact mode-specific context. Signature grouping carries deterministic
group counts, first-N sample review IDs, and score stats as presentation data
only; decisions still expand to individually hash-bound review IDs. Candidate-free
unmatched directional link items carry `right_surface_id: null` and allow only
defer actions. The HTML projection is static and offline; the canonical decision
data remain a deterministic JSON/CSV envelope that other frontends can produce.

`canon entity review import` keeps the positional decisions file. Non-session
imports require `--registry` and `--next-version`; session imports use
`--session` and `--reviewer` and take the registry binding from the session
manifest. Without
`--source-review`, it imports the existing queue decisions. With
`--source-review <canon_entity_native_review.v0>`, it treats the positional file
as native JSON or CSV decisions, expands JSON `group_decisions` against that
source review artifact, verifies the source review self-hash and exact
mode-context binding for every expanded member, and emits a typed
`canon_entity_native_review_import.v0` patch receipt only. The native path does
not read or mutate the registry, and does not consume `--audit` or `--next-version` beyond Clap-required
compatibility. For a derivation-proven collapse, a singleton cluster Alias
decision is valid only with an explicit target canonical ID and exact
exported-surface equality.

```bash
# Full pipeline in one command:
$ canon entity run rows.csv \
    --profile entity_profile \
    --strategy strategy.yaml \
    --registry registries/entities/ \
    --work-dir work/entity-run \
    --suite eval/holdout/ \
    --emit summary

entity_run: 847 rows → 312 canonical entities, 4 escrow (pending), 0 escrow (conflict)
audit: holdout 98/98 pass, perturbation stability 0.998
```

Or run stages individually for inspection:

```bash
$ canon entity prepare rows.csv --profile entity_profile --registry registries/entities/ --work-dir work/entity
$ canon entity index build rows.csv --profile entity_profile --strategy strategy.yaml --registry registries/entities/ --work-dir work/entity
$ canon entity block preflight rows.csv --profile entity_profile --strategy strategy.yaml --sample-pct 100 --emit summary
$ canon entity block rows.csv --profile entity_profile --strategy strategy.yaml --registry registries/entities/ --work-dir work/entity > blocks.jsonl
$ canon entity evidence rows.csv --profile entity_profile --strategy strategy.yaml --candidates blocks.jsonl --registry registries/entities/ --work-dir work/entity > evidence.jsonl
$ canon entity solve rows.csv --profile entity_profile --strategy strategy.yaml --evidence evidence.jsonl --registry registries/entities/ --work-dir work/entity > result.json
$ canon entity audit result.json --suite eval/holdout/ > audit.json
$ canon entity review export result.json --include all --emit csv > review.csv
$ canon entity review export result.json --artifact native-review --group-by signature --include all --emit html > native-review.html
$ canon entity review import review.csv --audit audit.json --registry registries/entities/ --next-version 2.1.0
$ canon entity promote result.json --audit audit.json --registry registries/entities/ --next-version 2.1.0
$ canon entity explain result.json --canon-id IC-00042
```

---

## The Entity Pipeline

### Strategy

Normalization views may declare the terminal-only `strip_footnote_marks` and
table-driven `strip_security_tail` steps before legal-form equivalence.
Footnotes such as `(14)`, `(a)(b)`, `#`, `^`, `~`, `*`, `†`, and superscript
digits are removed only at the end of a source name. Security tails are removed
only when a complete recognized phrase follows a separator; the removed tail
is retained as evidence metadata, while meaningful text such as `Fund III` and
`Class A` without a complete instrument phrase remains part of the name.

A normalization view may declare the `name_structure` operator. `canon entity session derive` then parses `X (dba Y)`, `X f/k/a Y`, `X aka Y` and member lists (`;`, ` / `, newline, or ` and ` / ` & ` only when every side ends in a legal form) out of one surface: a stated alternate becomes `asserted_alias` to an observation equal to the alternate or the primary, a list is `multi_party` and `related` (`co_borrower_of`) to its members and never `name_equivalent` with one, and `Johnson and Johnson` or `Smith, Jones and Partners LLP` stay one name.

`canon registry correct` also takes `{"op":"supersede","superseded","successor","succession":"merged_into","effective_date","inputs"}`:
the old id stays, answers lookups as itself with a `succession` block (`superseded_by`,
the `chain` to the active end), and the named spellings move to the successor; a name an earlier supersede placed on the old id follows without being listed again.
It needs a human reviewer, evidence ids and a date the evidence states.

A name shared by two entities (the same company name under two different CIKs, a
merger survivor that took its predecessor's name) blocks both from minting by
default (`key_conflict`). With `standalone.split_shared_names: true` in the rules,
`session solve` makes one entity per key value, writes the shared name as an alias
of none of them, and reports it under `shared_names`; one row carrying two keys, or
evidence joining the two, is still a `key_conflict`.

A unit inside a firm (a trust division, a servicing platform) is its own entity with a
`part_of` relation to the firm, never an alias of it. Stated on one surface as
`X, a division of Y`, the `name_structure` view makes `derive` relate X `part_of` Y.
`canon registry scope-policy --optional-dimension role` lets a registry hold a
spelling scoped to a role: an `asserted_alias` with `basis.scope {"role": R}` and
`basis.subject` (the observation whose spelling is scoped) is written at commit as an
entry scoped `role=R` on the unit's id, so `--scope role=R` returns the unit and a
bare lookup returns the firm. A role value alone scopes nothing, and a registry
without the policy refuses that commit (`scope_policy_missing`). `session solve`
reports `units` (evidence `surface_pattern`, `asserted_relation` or `distinct_key`)
and `scoped_aliases`.

A YAML file that configures the entire pipeline. Defines observation fields (`name_fields`, `anchor_fields`, `context_fields`), normalization views (lowercase, listed legal-form equivalence with `preserve_legal_form`, the separately declared `preserve_legal_form_with_bare` step, strip legal suffixes, extract initials), blocking operators, evidence rules, solver thresholds, reconciliation policy, and promotion gates. `preserve_legal_form` keeps distinct forms distinct (`LLC` is not `Inc`) while recording variant steps such as `legal_form:inc`; `preserve_legal_form_with_bare` additionally equates a bare name with exactly one unprotected trailing form and refuses ambiguous buckets containing multiple forms. Protected `Bank` and `Trust` tokens and mid-name `Co` in `Co-Invest` are retained.

### Block

Candidate neighborhood generation. Blocking operators reduce the O(n²) comparison space to plausible pairs:

| Operator | What it does |
|----------|-------------|
| `exact_view` | Blocks on exact match of a normalized name view |
| `rare_token_overlap` | Blocks on shared rare tokens weighted by IDF |
| `shared_anchor` | Blocks on shared anchor values (LEI, CIK, FIGI) |
| `registry_alias_match` | Blocks on existing registry alias matches |

### Evidence

Typed evidence scoring. Each candidate pair receives lane-tagged evidence:

- **Support** — scored positive evidence (exact name view match, acronym-plus-token, categorical field equality)
- **Trusted anchors** — explicit configured identifiers that can support equality when the profile says they are identity anchors
- **Cannot-link** — negative evidence (conflicting anchor values in the same namespace)
- **Relation hints** — relationship, hierarchy, or contextual evidence that may explain why records co-occur but must not silently become equivalence merge evidence

### Solve

Staged deterministic solver:

1. **Seed** — build initial components from must-link edges using union-find
2. **Backbone** — merge clusters via reciprocal best scoring pairs (requires positive name evidence, respects max cluster diameter)
3. **Attachment** — attach singletons to backbone clusters (requires winner margin, attachments don't chain)

**Reconciliation** then classifies each cluster:

- Single incumbent overlap → inherit existing canonical ID
- Multiple incumbent overlap → abstain with conflict escrow
- No incumbent → mint new canonical ID
- Low evidence → abstain with pending escrow

### Audit

Validate results against frozen evaluation suites. Checks holdout fixture pass rates and perturbation stability (strategy-configurable threshold, e.g. ≥ 0.995). Promotion requires a passing audit.

### Review

Export resolved, escrowed, or contradictory clusters into JSON or CSV review artifacts. Each item carries a stable review ID, source row IDs, observed names, anchors, incumbent overlaps, evidence scores, contradiction reasons, and a proposed action. Importing a reviewed artifact refuses malformed or duplicate decisions, stale registry snapshots, anchor conflicts, and alias/anchor promotion decisions without a matching audit.

### Promote

Write audited results back to the registry:

- Resolved entities get alias entries added to registry mapping files
- Escrow sidecars are written for entities that need human review
- Requires an explicit `--next-version` bump

### Explain

Proof traces for any row, entity, or escrow decision:

```bash
$ canon entity explain result.json --row src-row-42
$ canon entity explain result.json --canon-id IC-00042
$ canon entity explain result.json --escrow-id ESC-00007
```

Returns the full evidence chain: which blocking operator surfaced the pair, which evidence edges were scored, which solver stage produced the merge or abstention, and why.

---
