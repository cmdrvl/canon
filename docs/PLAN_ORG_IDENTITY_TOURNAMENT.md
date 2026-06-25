# canon org — Organization Identity Tournament

> **Status**: Draft
> **Created**: 2026-03-22
> **Revised**: 2026-05-06
> **Successor direction**: `PLAN_ENTITY_WORKBENCH.md` generalizes this workbench
> from `canon org` to `canon entity`, adds native Rust namekit primitives,
> first-class anti-merge evidence, and the first non-org profile
> (`cmbs_tenant_label`). This document remains the legacy source of truth for
> the currently implemented org-specific workbench until direct replacement; no
> long-lived `canon org` compatibility alias is planned.
> **Context**: The parsing tournament can emit a normalized row relation with
> stable `source_row_id` and reusable semantic fields. The next compounding
> layer is organization identity: repeated organization-like names across
> filings with noisy aliases and sparse external anchors. The v1 validated scope
> is BDC portfolio-company issuer identity, but the CLI and artifact contracts
> should remain domain-generic.

---

## One-line thesis

Give `canon` a normalized row corpus and a promotable org strategy. It builds an
ephemeral evidence graph, resolves stable organization identities
conservatively, emits proof-carrying clusters and registry patches, and lets the
tournament mutate the strategy instead of humans maintaining alias sheets by
hand.

---

## Why this exists

`canon` v0 resolves names only when someone has already authored the registry.
That is useful, but it leaves a large manual step untouched:

- BDC portfolio companies recur quarter after quarter under slightly different
  names
- other organization-heavy domains may later need the same machinery
- external anchors such as LEI, FIGI, or CIK are sparse and uneven
- the organization name is often the only reusable identity surface

Today this work lives in:

- analyst-maintained alias tabs
- ad hoc pairwise spreadsheets
- codebook drift in downstream models
- one-off manual cleanups that do not compound

The organization tournament turns that work into a deterministic, promotable,
and compounding artifact.

---

## Relationship to existing plans

This plan sits cleanly beside the current docs rather than replacing them.

### Relationship to `IDENTITY_ARCHITECTURE.md`

`canon org` is the first implemented resolution workbench in the shared identity
architecture. It is allowed to run blocking, evidence scoring, cluster solving,
abstention, audit, review, and promotion because those steps happen outside the
core exact lookup kernel. The durable result is still registry knowledge that
ordinary `canon` lookup can consume exactly.

### Relationship to `PLAN_BDC_ENTITY_REGISTRATION.md`

That plan is the bootstrap version for BDC issuer identity:

- one tape
- normalization-heavy
- deterministic clustering by normalized key

This plan broadens the model:

- normalization becomes candidate generation, not the whole resolver
- the resolver is a constrained evidence graph
- the public interface stays generic even though only one domain profile is
  validated at first

The BDC issuer plan becomes the first validated domain profile for this engine.

### Relationship to `PLAN_STRUCTURAL_RESOLUTION.md`

That plan is for matching **records across tapes**.

This plan is for resolving **organizations across many observations**.

Cross-tape record matching remains a separate layer. The organization identity
layer may feed it, but does not replace it.

### Relationship to the tournament identity model

The identity split remains:

1. `source_row_id`
2. `org_canon_id`
3. `position_match_id`

`canon org` owns the middle layer only. Individual profiles may interpret
`org_canon_id` more specifically, for example as issuer identity.

---

## Design principles

1. **Canon stays a tool, not a loop.** `canon org` runs one deterministic
   strategy against one frozen corpus and emits structured artifacts. The
   tournament lives outside the binary.

2. **The graph is ephemeral.** The evidence graph is hydrated in memory for one
   run, solved, and torn down. The persistent asset is the registry plus
   sidecar proofs and escrow memory, not a live graph database.

3. **The registry stays flat.** The lookup fast path remains `input ->
   canonical_id`. Richer evidence and escrow memory live in ignored sidecars
   under underscore paths.

4. **Abstention is first-class.** The system does not have to merge every
   observation. High-confidence promotions compound; low-confidence cases wait
   for later evidence.

5. **Abstention compounds through escrow.** Unresolved-but-promising evidence is
   persisted as provisional memory so later filings can promote it without human
   relabeling.

6. **Internal IDs come first.** Stable internal organization IDs are primary.
   External references are attached when available, not forced as the identity
   system.

7. **The interface stays generic; validation stays narrow.** Commands, schemas,
   and registry formats should not hardcode BDC terms, but v1 should make no
   claim of cross-domain quality beyond the BDC issuer profile.

8. **Strategies are promotable artifacts.** The mutable surface is one bounded
   strategy YAML. The tournament mutates that artifact and nothing else by
   default.

9. **Deterministic given the same inputs.** Same observations + same strategy +
   same incumbent registry and escrow sidecars = same candidates, same graph,
   same clusters, same promotions, same escrow updates.

10. **Evidence must be inspectable.** Every merge, abstention, contradiction,
    promotion, and escrow update needs a machine-readable reason trail.

---

## Non-goals

This plan is NOT:

- a runtime LLM entity linker
- an always-on graph database
- a universal investment ID system
- a replacement for cross-quarter economic-position matching
- an attempt to solve every open-world entity ambiguity automatically
- a fuzzy black box with opaque embeddings as the production decision rule

It may use model-backed mutation in the tournament runner, but not in the
deterministic CLI execution path.

---

## CLI surface

The namespace should be `canon org`, not `canon register`, because the engine
should work across organization-like domains.

```bash
canon org run <ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY_DIR> \
  [--suite <SUITE_DIR>] [--emit json|summary] [--no-witness]

canon org block <ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY_DIR> \
  [--emit jsonl|summary]

canon org edge <ROWS> --strategy <STRATEGY.yaml> --candidates <BLOCK.jsonl> \
  --registry <REGISTRY_DIR> [--emit jsonl|summary]

canon org solve <ROWS> --strategy <STRATEGY.yaml> --edges <EDGES.jsonl> \
  --registry <REGISTRY_DIR> [--emit json|summary]

canon org audit <RESULT.json> --suite <SUITE_DIR> [--emit json|summary]

canon org promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY_DIR> \
  --next-version <VERSION> [--emit json|summary]

canon org review export <RESULT.json> [--emit json|csv] \
  [--include resolved|escrow|contradictions|all]

canon org review import <REVIEW.json|csv> --registry <REGISTRY_DIR> \
  --next-version <VERSION> [--audit <AUDIT.json>] [--emit json|summary]

canon org explain <RESULT.json> --row <SOURCE_ROW_ID>|--canon-id <CANON_ID>|--escrow-id <ESCROW_ID> \
  [--emit json|summary]
```

### Command roles

- `run`: orchestration wrapper for production and challenger execution
- `block`: candidate neighborhood generation
- `edge`: typed evidence generation
- `solve`: deterministic graph partition and registry inheritance
- `audit`: scoring against frozen evaluation suites
- `promote`: safe registry and escrow write-back
- `review`: deterministic human adjudication export/import for unresolved,
  abstained, contradictory, or operator-selected resolved clusters
- `explain`: proof trace for one row or one canonical entity

`run` is what production and tournaments normally call. The others exist to keep
the artifact boundaries explicit and testable.

### Refusal semantics

`canon org` should reuse the normal `canon` refusal envelope and exit semantics.

Refusal is appropriate when the tool cannot evaluate the org strategy at all. It
is not the same thing as abstention. Abstention is a domain outcome inside a
successful solve; refusal is an operator handoff.

Minimum org-specific refusal cases:

- missing required row fields for the active strategy
- malformed structured side fields such as `alias_surfaces_json`
- malformed or semantically invalid strategy YAML
- suite/profile mismatch
- fixture references to unknown `source_row_id` values
- unsupported emit mode for the chosen command
- promotion requested without an explicit next registry version
- promotion requested against a registry snapshot that no longer matches the
  audited result artifact
- review import with malformed or duplicate decisions, stale registry snapshots,
  trusted-anchor conflicts, or alias/anchor promotion decisions without a
  matching audit artifact

Suggested refusal codes:

- `E_ORG_INPUT_CONTRACT`
- `E_ORG_BAD_STRATEGY`
- `E_ORG_BAD_SUITE`
- `E_ORG_FIXTURE_INVALID`
- `E_ORG_VERSION_BUMP_REQUIRED`
- `E_ORG_STALE_REGISTRY`

---

## Input contract

### Primary input: normalized rows

The input is a CSV or JSONL relation close to the normalized row model used by
the parsing tournament.

#### Required row fields

At minimum, every row must provide:

- `source_row_id`
- `doc_id`
- `as_of_date`
- one primary organization-name surface

For the validated BDC issuer profile, the primary name surface is normally
`portfolio_company`.

#### Recommended row fields

These fields materially improve BDC issuer resolution and should be treated as
part of the expected v1 normalized-row contract even if they are not globally
required by the generic interface:

- `portfolio_company`
- `investment_type`
- `industry`
- `interest_rate`
- `maturity_date`
- `par_amount`
- `fair_value`
- `cost`
- `label_notes_json`

#### Optional structured side fields

These are optional, but if present they should already be parser-produced and
deterministic:

- `alias_surfaces_json`
- `mention_surfaces_json`
- `lei`
- `cik`
- `figi`
- `filer_id`
- `row_index`

Field semantics:

- `alias_surfaces_json` is a JSON array of strings
- `mention_surfaces_json` is a JSON array of strings
- `lei`, `cik`, and `figi` are scalar namespace values, not embedded JSON blobs
- `row_index` is the stable row ordinal within one parser-emitted document view

The strategy declares which columns matter and how they are interpreted. The org
resolver must not perform open-ended parsing of raw filing text to manufacture
these fields for itself.

#### Stability requirements

`source_row_id` and `doc_id` must be stable enough to serve as fixture keys and
registry history keys.

Rules:

- `source_row_id` is immutable for a given normalized-row emission of one filing
- `source_row_id` must not depend on local filesystem path, run timestamp, or
  strategy choice
- `doc_id` must identify the economic source document, not one ephemeral file
  path
- `as_of_date` must be emitted in one canonical date format

For BDC v1, a change in parser formatting that preserves the same row semantics
should preserve both `source_row_id` and `doc_id`. If those IDs churn, the org
tournament and audit harness become invalid.

#### Row semantics

One normalized row should correspond to one disclosed schedule line item or one
parser-defined carried-through blank line. The org resolver should not combine
or split rows before the projection step.

V1 validation assumes the BDC issuer profile, so the first shipped strategies
and suites will expect BDC-like surfaces even though the contract remains
generic.

### Org projection layer

`canon org` should build an explicit internal projection from normalized rows
before blocking. This is not a separate CLI subcommand in v0, but it is a real
contract inside the implementation.

Each projected observation should contain:

- `source_row_id`
- `doc_id`
- `as_of_date`
- `primary_surface`
- `alias_surfaces[]`
- `mention_surfaces[]`
- `anchors[]`
- `context`
- `provenance`

Projection rules:

- `primary_surface` comes from one declared name field
- `alias_surfaces[]` may come from parser-emitted structured side fields or
  deterministic note extraction only
- `mention_surfaces[]` may come only from parser-emitted structured mentions
- `anchors[]` are normalized namespace/value pairs
- `context` is a typed map of declared strategy fields
- `provenance` records which original columns contributed which projected values

No projection rule may invent a surface not traceable to one explicit input
field.

If `alias_surfaces_json` or `mention_surfaces_json` is missing, malformed, or
contains non-string members, projection should behave as follows:

- if the field is absent, treat it as an empty list
- if the field is present but malformed, refuse with the input-contract refusal
- if the active strategy lists the field under
  `observations.required_side_fields` and it is absent, refuse with the
  input-contract refusal

In v1, `observations.required_side_fields` may name only:

- `alias_surfaces_json`
- `mention_surfaces_json`

#### Internal projection artifact

The implementation should use a typed internal projection artifact, effectively
`canon_org_projection.v0`, even if it is not exposed as a public CLI output in
v0.

Suggested shape:

```json
{
  "version":"canon_org_projection.v0",
  "source_row_id":"row-1",
  "doc_id":"0000000000-25-000001",
  "as_of_date":"2025-12-31",
  "primary_surface":{"value":"Acme Corp.","field":"portfolio_company"},
  "alias_surfaces":[{"value":"ACME Corporation","field":"alias_surfaces_json"}],
  "mention_surfaces":[{"value":"Acme","field":"mention_surfaces_json"}],
  "anchors":[{"namespace":"lei","value":"549300...","field":"lei"}],
  "context":{
    "industry":"Software",
    "investment_type":"First Lien"
  }
}
```

This projection layer is the only place where row columns are decoded into org
observations. Downstream stages should consume projected observations, not raw
CSV/JSONL cells.

### Incumbent registry

The registry directory is the same concept as normal `canon`:

- existing aliases can resolve immediately
- promoted aliases are written back in the same flat format
- sidecars record proof and anchor details

---

## How it works

### Pipeline

```text
normalized rows + incumbent registry + escrow sidecars
  |
  v
1. Project row surfaces and context per strategy
2. Normalize views (candidate-generation only)
3. Build candidate neighborhoods
4. Emit must-link / support / cannot-link evidence edges
5. Solve graph conservatively
6. Reconcile clusters with incumbent registry
7. Optionally attach external references
8. Audit against frozen suite
9. Derive safe registry write-back
10. Derive safe escrow write-back
```

### Step 1: projection

The strategy selects:

- organization name surfaces
- optional alias-bearing notes
- anchor fields
- context fields
- contradiction fields

### Step 2: normalized views

Multiple views may be derived from the same surface:

- core name
- acronym
- rare tokens
- anchor namespace values

These views are used for blocking and evidence, not as the final identity rule.

### Step 3: candidate neighborhoods

Blocking narrows the search space before graph construction.

Examples:

- exact normalized core-name match
- rare-token overlap
- shared anchor
- same deal or same filer family context
- explicit registry alias match
- escrow hypothesis surface or anchor match

### Step 4: evidence edges

Each candidate pair may generate:

- `must_link` edges: hard positive evidence
- `support` edges: soft positive evidence with deterministic score
- `cannot_link` edges: hard negative evidence

Every edge records:

- source row IDs
- operator ID
- evidence namespace
- score or boolean result
- rendered explanation line

### Step 5: solve

The solver must be deterministic and conservative. For the validated BDC issuer
profile, it should not behave like a generic graph partitioner. It should run as
a staged cluster builder:

1. collapse must-link edges into atomic seed components
2. reject any seed component that already contains hard contradictions
3. aggregate seed-to-seed support by namespace, capping each namespace to one
   contribution so many weak context matches cannot swamp name evidence
4. merge seeds into backbone clusters only when they are reciprocal best
   partners, clear `backbone_score_min`, and do not violate cluster invariants
5. attach peripheral nodes or components only when they carry positive name
   evidence, clear `attach_score_min`, beat the next-best candidate by margin,
   and connect to at least one backbone member
6. abstain on anything that remains weak, ambiguous, or contradictory

The solver is a constrained seed -> backbone -> attachment pipeline with
explicit abstention.

#### Fixed support namespaces

For the validated BDC issuer profile, every positive evidence operator must map
into one of a small fixed set of namespaces:

- `registry`
- `anchor`
- `name`
- `context`
- `temporal`

Pair support is then aggregated deterministically as:

```text
score_ns(A, B) = max(positive operator scores in namespace ns)
score(A, B) = sum(score_ns(A, B) for all namespaces)
```

This is intentionally simple. Multiple weak operators in the same namespace do
not stack without bound.

#### Component-level scoring

Let `pair_score_ns(x, y)` be the per-namespace score for observation pair
`(x, y)` after operator evaluation, and let `pair_score(x, y)` be the namespace
sum defined above.

For any two solver components `A` and `B`, define:

```text
witness_pair(A, B) =
  argmax(pair_score(x, y)) for x in core(A), y in core(B)

component_score_ns(A, B) =
  pair_score_ns(witness_pair(A, B))

component_score(A, B) =
  pair_score(witness_pair(A, B))
```

`score_name(A, B)` means `component_score_name(A, B)`, and similarly
`score(P, C)` means `component_score(P, C)` when either side is a solver
component rather than one raw observation.

Where:

- `core(A)` means the seed members of `A` if `A` is still unresolved
- `core(A)` means only backbone members of `A` if `A` already has attachments
- attached members never contribute to later merge or attachment scoring

This rule is deliberate. The backbone is built from the strongest direct
cross-component witness pair among core members only; fringe attachments are
never allowed to create new merge opportunities.

This is stricter than taking the best score separately in each namespace across
different member pairs. It prevents a cluster merge from being justified by a
synthetic combination such as "name evidence from one pair plus context evidence
from another pair."

#### Backbone merge rule

Two seed components `A` and `B` may merge into the same backbone cluster only if
all of the following hold:

- no hard negative relation exists between any members of `A` and `B`
- `score_name(A, B) > 0`
- `score(A, B) >= backbone_score_min`
- `B` is the best eligible merge partner for `A`
- `A` is the best eligible merge partner for `B`
- the merged cluster still satisfies cluster invariants

If multiple partners tie on score, ties are broken deterministically by source
order.

Backbone merges are applied greedily in descending score order, with candidate
scores recomputed after each accepted merge using only backbone members. A later
merge is legal only if it still satisfies the full backbone rule against the
current merged cluster.

#### Attachment rule

A peripheral component `P` may attach to an existing backbone cluster `C` only
if all of the following hold:

- no hard negative relation exists between `P` and any member of `C`
- `score_name(P, C) > 0`
- `score(P, C) >= attach_score_min`
- `score(P, C) - score(P, C_second_best) >= abstain_margin`
- `P` has direct positive evidence to at least one backbone member of `C`
- attaching `P` does not violate cluster invariants

Attachments must not bootstrap more attachments during the same solver pass. In
other words, newly attached members do not become the sole justification for
attaching later members.

If `P` is itself a multi-row seed component, the attachment score is computed
against backbone members of `C` only. Members already attached to `C` are
ignored for scoring purposes.

If no second-best eligible cluster exists for `P`, define `score(P,
C_second_best) = 0`.

#### Cluster invariants

The BDC issuer solver should enforce at least these invariants:

- no conflicting trusted anchors inside one cluster
- no merge created by context-only evidence without positive name evidence
- every attached member must be justified by a path to a backbone member
- promotable new clusters must have either one high-trust anchor or observations
  from multiple distinct documents
- attached members must never be the sole witness for a later backbone merge

#### Independent observation rule

For BDC issuer promotion, "independent observations" means distinct `doc_id`
values, not multiple rows from the same filing. Multiple rows in one schedule
may help solve a cluster, but they should not by themselves justify promotion of
a brand-new canonical issuer.

### Step 6: reconcile with incumbent registry

Each solved component becomes one of:

- `RESOLVED_EXISTING`
- `PROMOTABLE_NEW`
- `ABSTAIN_LOW_EVIDENCE`
- `ABSTAIN_CONFLICT`

Define `incumbent_ids(K)` as the set of existing `org_canon_id` values touched
by registry-backed aliases inside solved component `K`.

Reconciliation rules for the validated BDC issuer profile:

1. if `incumbent_ids(K) = {}`:
   the component is handled as a new-cluster candidate and may become
   `PROMOTABLE_NEW` or an abstention
2. if `incumbent_ids(K) = {id}` and no trusted-anchor conflict exists:
   the component becomes `RESOLVED_EXISTING` and inherits `id`
3. if `incumbent_ids(K)` contains more than one distinct ID:
   the component becomes `ABSTAIN_CONFLICT`
4. if `incumbent_ids(K) = {id}` but trusted-anchor evidence conflicts with that
   incumbent:
   the component becomes `ABSTAIN_CONFLICT`

V1 must not automatically merge incumbent canonical IDs. If a solved component
touches multiple incumbent IDs, `canon org` must abstain rather than pick a
winner or rewrite identity history.

#### Existing-ID alias expansion

`RESOLVED_EXISTING` components may still propose new alias write-back for the
inherited ID, but only for aliases that are safe under the same hard gates.

V1 should be conservative:

- an alias observed only through a weak attachment should not be written back
- aliases supported by backbone members are eligible
- a one-document alias may be written back only when the resolved-existing
  cluster is anchored by a trusted unique anchor or the alias already appears in
  the incumbent registry neighborhood

### Step 7: external references

External references are attached if strong enough, but do not override the
internal identity rules.

Examples:

- unique LEI agreement
- unique CIK agreement
- FIGI-derived issuer name agreement

#### Anchor trust policy

Anchor trust must be profile-defined rather than assumed globally.

For the validated BDC issuer profile:

- `lei` is a trusted anchor and may participate in `must_link` and
  single-document promotion when unique and conflict-free
- `cik` is support evidence by default, not a standalone `must_link`, unless the
  upstream parser can prove it is a direct issuer-level reference
- `figi` is support-only for issuer identity in v1 and must not by itself create
  a new issuer cluster or promotion

This is intentionally conservative. In BDC data, the cost of a false issuer
merge is much higher than the cost of waiting for another quarter of evidence.

### Enrichment stance

Enrichment is helpful, but it is not required for v1 correctness.

`canon org` should not perform live network lookups, open-ended retrieval, or
free-text filing search in the core execution path.

Allowed enrichment for v1:

- structured anchors already present in the normalized rows
- deterministic alias-bearing notes or mention surfaces extracted upstream by
  the parser
- local snapshot joins from pinned external datasets that are loaded as explicit
  side inputs

Enrichment should be treated as additional evidence, not as an identity
override. A cluster should not be promoted solely because one opaque enrichment
source suggested it.

### Step 8: audit

The run is scored against a frozen suite containing:

- optional gold pairs or gold clusters
- silver anchor truths
- perturbation sets
- contradiction fixtures
- holdout corpora
- downstream continuity proxy slices

### Step 9: promote

Only safe aliases and safe new IDs are written back to the registry.

Abstentions are not promoted.

#### Promotion eligibility

For the validated BDC issuer profile, a cluster is `PROMOTABLE_NEW` only if all
of the following hold:

- it does not cleanly inherit one incumbent `org_canon_id`
- it contains no trusted-anchor conflict
- it satisfies the suite hard gates
- it has either one unique high-trust anchor or support from at least
  `min_distinct_docs` distinct `doc_id` values
- it contains at least one backbone relation, unless it is a single-observation
  anchored cluster

Anything else must remain unresolved for write-back purposes, even if the run
keeps the cluster internally for explanation and audit.

#### Existing-ID alias write-back eligibility

For a `RESOLVED_EXISTING` cluster, new alias write-back is allowed only if all
of the following hold:

- the cluster inherited exactly one incumbent `org_canon_id`
- no trusted-anchor conflict exists
- the alias is supported by at least one backbone member of the resolved cluster
- the audited result satisfies the suite hard gates

This separates "inherit an existing identity" from "teach the registry a new
alias for that identity."

`canon org promote` must require a matching audit artifact. Promotion is allowed
only if:

- `audit.summary.hard_gates_passed = true`
- `audit.summary.decision = "PROMOTE"`
- the audit artifact is for the exact result artifact being promoted
- the current registry and escrow snapshot under `--registry` matches the
  result artifact's recorded `lookup_snapshot_hash` and
  `escrow_snapshot_hash`
- `--next-version` is provided and differs from the current
  `registry.json.version`

Because `canon` treats registry version as part of the resolution contract,
promotion may not silently mutate flat lookup mappings under an unchanged
registry version.

### Step 10: identity escrow

Identity escrow is the provisional-memory layer for abstained or unresolved
identity work.

Escrow is not canonical truth:

- it does not mint `org_canon_id`
- it does not satisfy `RESOLVED_EXISTING`
- it does not override hard gates

It does persist evidence that would otherwise be lost between quarters.

#### Escrow record types

For the validated BDC issuer profile, v1 should persist at least:

- `pending_cluster`
- `cannot_link_fact`
- trusted anchor observations already covered by `_anchors/`

#### Pending-cluster eligibility

An `ABSTAIN_LOW_EVIDENCE` component is escrow-eligible only if all of the
following hold:

- it contains positive name evidence
- it contains no trusted-anchor conflict
- it does not overlap multiple incumbent `org_canon_id` values
- it has at least one backbone relation or one unique high-trust anchor

#### Escrow upsert rule

An escrow-eligible component should upsert one existing `pending_cluster` only
if all of the following hold:

- there is no trusted-anchor conflict with that pending cluster
- the best escrow match has positive name evidence
- the best escrow match beats the next-best escrow match by the same
  `abstain_margin` logic used for live attachments

Otherwise, the component should create a new provisional `escrow_id`.

#### Conflict escrow

An `ABSTAIN_CONFLICT` component should not create a pending-cluster escrow
hypothesis, but it may emit durable `cannot_link_fact` records and trusted-anchor
observations for future runs.

#### How future runs may use escrow

Escrow may be used only for:

- candidate expansion during blocking
- additional support for distinct-document accumulation
- reuse of prior `cannot_link` facts
- proof trails in `explain`

Escrow may not:

- create `must_link` by itself
- resolve a row directly to `RESOLVED_EXISTING`
- override a trusted-anchor conflict
- cause incumbent canonical IDs to merge

#### Escrow promotion rule

If a future run plus escrow memory satisfies the normal hard gates for a new
entity, the pending hypothesis may be promoted to a real `org_canon_id`. Until
then it remains provisional.

---

## Primitive operator library

All operators are pure functions. The tournament mutates which operators are
enabled, how they are ordered, and how they are weighted.

### Validated v1 basis

For the validated BDC issuer profile, the actual operator basis should stay
small.

Required normalize operators:

- `lowercase`
- `strip_footnotes`
- `strip_legal_suffixes`
- `normalize_whitespace`
- `extract_initials`
- `tokenize`
- `drop_stopwords`

Required blocking operators:

- `exact_view`
- `rare_token_overlap`
- `shared_anchor`
- `registry_alias_match`

Required evidence operators:

- `shared_anchor`
- `conflicting_anchor`
- `exact_view`
- `acronym_plus_token`
- `categorical_equal`
- `registry_alias_match`

Anything outside that basis should be treated as future or experimental, not as
part of the validated BDC v1 search space.

### Normalize operators

- `lowercase`
- `ascii_trim`
- `strip_footnotes`
- `strip_parenthetical_notes`
- `strip_legal_suffixes`
- `normalize_whitespace`
- `extract_initials`
- `tokenize`
- `drop_stopwords`

### Blocking operators

- `exact_view`
- `prefix_view`
- `rare_token_overlap`
- `shared_anchor`
- `registry_alias_match`
- `same_namespace_value`

### Evidence operators

- `shared_anchor`
- `conflicting_anchor`
- `exact_view`
- `acronym_plus_token`
- `categorical_equal`
- `set_overlap`
- `numeric_tolerance_pct`
- `numeric_tolerance_abs`
- `date_range`
- `registry_alias_match`
- `shared_neighborhood`
- `repeated_cooccurrence`

### Minimal parameter contracts

To keep strategy mutation bounded, the validated v1 operators should accept only
simple parameter shapes:

- `exact_view`
  required: `view`
- `rare_token_overlap`
  required: `left_view`, `right_view`, `min_tokens`, `min_idf`
- `shared_anchor`
  required: `anchor`
- `conflicting_anchor`
  required: `anchor`
- `registry_alias_match`
  required: none
- `acronym_plus_token`
  required: `acronym_view`, `token_view`, `score`
- `categorical_equal`
  required: `field`, `score`

V1 should not allow arbitrary embedded expressions, custom regex bodies inside
the strategy, or user-defined operators.

### Solver policies

- must-link-first backbone
- namespace-capped support aggregation
- reciprocal-best backbone merges
- winner-margin attachments
- deterministic tie-break by score then source order
- abstain margin between best and next-best attachment
- max cluster diameter or attachment depth
- require positive name evidence for attachment

In v0, prefer explicit operators over learned embeddings. If the primitive basis
plateaus, new operators can later enter via bounded code-patch mutation, but
that is explicitly outside the validated BDC v1 basis.

---

## Strategy schema

```yaml
strategy_id: bdc_org_graph.v1
strategy_version: "0.1.0"
entity_type: issuer
description: "Resolve BDC portfolio-company identities via constrained evidence graph"
id_prefix: "IC"

observations:
  name_fields: [portfolio_company]
  required_side_fields: []
  context_fields: [industry, investment_type, interest_rate, maturity_date, par_amount]
  anchor_fields:
    lei: lei
    figi: figi
    cik: cik

normalize:
  views:
    core_name:
      - lowercase
      - strip_footnotes
      - strip_legal_suffixes
      - normalize_whitespace
    acronym:
      - extract_initials
    rare_tokens:
      - tokenize
      - drop_stopwords

blocking:
  - op: exact_view
    view: core_name
  - op: rare_token_overlap
    left_view: rare_tokens
    right_view: rare_tokens
    min_tokens: 2
    min_idf: 4.0
  - op: shared_anchor
    anchor: lei
  - op: registry_alias_match

evidence:
  must_link:
    - op: shared_anchor
      anchor: lei
    - op: registry_alias_match
  support:
    - op: exact_view
      view: core_name
      score: 32
    - op: acronym_plus_token
      acronym_view: acronym
      token_view: rare_tokens
      score: 10
    - op: categorical_equal
      field: industry
      score: 4
    - op: categorical_equal
      field: investment_type
      score: 3
  cannot_link:
    - op: conflicting_anchor
      anchor: lei

solver:
  score_mode: namespace_max_sum
  component_score_mode: core_best_pair_sum
  merge_policy: reciprocal_best
  backbone_score_min: 32
  backbone_requires_positive_name: true
  attach_score_min: 28
  abstain_margin: 6
  max_cluster_diameter: 2
  require_positive_name_evidence: true
  attach_requires_backbone_contact: true
  score_against_backbone_only: true
  attachments_do_not_chain: true

reconcile:
  single_incumbent_overlap: inherit
  multi_incumbent_overlap: abstain_conflict
  allow_incumbent_merge: false
  allow_alias_writeback_for_resolved_existing: true

anchors:
  precedence: [lei, cik, figi]
  trusted_for_must_link: [lei]
  trusted_for_single_doc_promotion: [lei]
  support_only: [cik, figi]
  require_unique_for_attachment: true

promotion:
  write_states: [PROMOTABLE_NEW, RESOLVED_EXISTING]
  require_zero_anchor_conflicts: true
  require_holdout_non_regression: true
  require_perturbation_stability_gte: 0.995
  min_distinct_docs: 2
  allow_single_doc_if_unique_anchor: true
```

### Mutation surface

The tournament may mutate:

- normalize view composition
- blocking rule enablement, order, thresholds
- enabled evidence operators
- support scores
- cannot-link rules
- solver thresholds and abstain margins
- anchor precedence
- anchor trust classifications
- promotion gates

The tournament must not mutate:

- target schema
- frozen corpora
- verify logic
- audit metric definitions
- registry history after seeing holdout
- the no-incumbent-merge safety rule
- the validated v1 operator basis, unless the run is in an explicit post-plateau
  code-patch phase

---

## ID policy

### Internal IDs are persistent

Stable organization IDs are preserved through registry inheritance.

Once an `org_canon_id` exists in the registry, later runs may expand its alias
set but must not silently replace it with a new ID.

### Escrow IDs are provisional

Escrow hypotheses use provisional `escrow_id` values, for example `OE-*`.

An `escrow_id` is stable across future escrow updates, but it is not a canonical
identity and must never appear in the flat lookup registry.

When a new pending cluster is created, its `escrow_id` should be derived
deterministically from the earliest witness tuple that justified escrow creation
plus the earliest supporting `doc_id`.

### New IDs are minted only at promotion

The solver may use an ephemeral cluster fingerprint during one run. Stable IDs
are assigned only when a cluster is promoted.

V1 mints IDs only for `PROMOTABLE_NEW` clusters. `RESOLVED_EXISTING` clusters
inherit one incumbent ID or abstain; they do not mint replacements.

### Minting precedence

1. If the cluster overlaps one existing canonical ID without contradiction,
   inherit it
2. Else if one unique high-trust external anchor exists, derive the seed from
   that anchor tuple
3. Else derive the seed from the sorted backbone witness aliases used to justify
   promotion

If the cluster overlaps multiple incumbent IDs, do not mint. Abstain.

The important property is not the literal seed function. It is that stable IDs
are not tied only to one normalized name string.

---

## Output contracts

### Ordering rules

All intermediate artifacts must be deterministic and stable under re-run:

- candidate pairs are canonicalized so `left_row_id < right_row_id`
- `block` and `edge` JSONL records are sorted lexicographically by
  `(left_row_id, right_row_id)`
- solved entities are ordered by state, then inherited `org_canon_id` if
  present, then first backbone row ID
- row lists inside one entity are sorted lexicographically

Whenever an artifact carries strategy or registry provenance:

- `strategy.content_hash` is the BLAKE3 hash of the exact UTF-8 bytes of the
  strategy YAML used to produce the artifact
- `lookup_snapshot_hash` is the BLAKE3 hash of a deterministic manifest over
  `registry.json`, all consulted flat mapping files in lexicographic order, and
  any `_anchors/` sidecars consulted by org resolution
- `escrow_snapshot_hash` is the BLAKE3 hash of a deterministic manifest over
  consulted `_escrow/` sidecars

### `canon_org_block.v0` (JSONL)

`canon org block` emits one candidate-pair record per line. Each pair appears at
most once.

`block` is registry-aware in v0. It may consult incumbent alias mappings,
trusted-anchor sidecars, and escrow hypotheses under the provided
`--registry <REGISTRY_DIR>`.

Suggested shape:

```json
{"version":"canon_org_block.v0","strategy":{"id":"bdc_org_graph.v1","content_hash":"blake3:..."},"registry_snapshot":{"registry_id":"bdc-issuers","registry_version":"2026.03.01","source":"registries/bdc-issuers/","lookup_snapshot_hash":"blake3:...","escrow_snapshot_hash":"blake3:..."},"left_row_id":"row-1","right_row_id":"row-9","block_hits":[{"operator_id":"exact_view:core_name"},{"operator_id":"registry_alias_match"}]}
```

Required fields:

- `version`
- `strategy`
- `registry_snapshot`
- `left_row_id`
- `right_row_id`
- `block_hits[]`

`block_hits[]` records only why the pair survived blocking. It is not yet
scored evidence.

`canon org edge` must refuse if the block artifact's `strategy.content_hash` or
`registry_snapshot` does not match the active `--strategy` and `--registry`
inputs.

### `canon_org_edge.v0` (JSONL)

`canon org edge` emits one scored pair-evidence record per candidate pair.

`edge` is registry-aware in v0 for the same reason as `block`: pair evidence may
depend on incumbent alias matches, trusted-anchor history, and reusable
cannot-link facts from escrow sidecars.

Suggested shape:

```json
{
  "version":"canon_org_edge.v0",
  "strategy":{"id":"bdc_org_graph.v1","content_hash":"blake3:..."},
  "registry_snapshot":{"registry_id":"bdc-issuers","registry_version":"2026.03.01","source":"registries/bdc-issuers/","lookup_snapshot_hash":"blake3:...","escrow_snapshot_hash":"blake3:..."},
  "left_row_id":"row-1",
  "right_row_id":"row-9",
  "hits":[
    {"kind":"must_link","namespace":"registry","operator_id":"registry_alias_match","score":0,"explanation":"registry alias match"},
    {"kind":"support","namespace":"name","operator_id":"exact_view:core_name","score":32,"explanation":"core_name exact match"}
  ],
  "pair_score_by_namespace":{"registry":0,"name":32},
  "pair_score_total":32,
  "has_must_link":true,
  "has_cannot_link":false
}
```

Required fields:

- `version`
- `strategy`
- `registry_snapshot`
- `left_row_id`
- `right_row_id`
- `hits[]`
- `pair_score_by_namespace`
- `pair_score_total`
- `has_must_link`
- `has_cannot_link`

`hits[]` is the evidence source of truth for `solve` and `explain`.

`canon org solve` must refuse if the edge artifact's `strategy.content_hash` or
`registry_snapshot` does not match the active `--strategy` and `--registry`
inputs.

### `canon_org_solve.v0` and `canon_org_run.v0`

In v0, `canon org solve` and `canon org run` share the same payload shape. The
only required difference is the top-level `version` string.

`solve` may emit `canon_org_solve.v0`; `run` may emit `canon_org_run.v0`.

`canon org audit`, `canon org promote`, and `canon org explain` should accept
either payload in v0.

Required top-level fields:

- `version`
- `strategy`
- `registry`
- `summary`
- `entities`
- `abstentions`
- `contradictions`
- `proposed_registry_patch`
- `proposed_escrow_patch`

Each entity record should expose enough structure for deterministic explain and
write-back:

- `state`
- `canonical_id`, when inherited or minted
- `backbone_rows`
- `attached_rows`
- `all_rows`
- `aliases`
- `anchors`
- `merge_witnesses[]`
- `inheritance`
- `eligible_writeback_aliases[]`
- `escrow`, when applicable

Where:

- `merge_witnesses[]` references the specific row pairs from `canon_org_edge.v0`
  that justified backbone merges
- `inheritance` explains whether the cluster inherited one incumbent ID,
  remained new, or abstained due to multiple incumbent IDs
- `escrow` records provisional escrow actions such as `UPSERT_PENDING` or
  `EMIT_CANNOT_LINK`
- `registry.lookup_snapshot_hash` and `registry.escrow_snapshot_hash` bind the
  result artifact to the exact registry and escrow memory it used

### Example `canon_org_run.v0`

```json
{
  "version": "canon_org_run.v0",
  "strategy": {
    "id": "bdc_org_graph.v1",
    "version": "0.1.0",
    "content_hash": "blake3:..."
  },
  "registry": {
    "id": "bdc-issuers",
    "version": "2026.03.01",
    "source": "registries/bdc-issuers/",
    "lookup_snapshot_hash": "blake3:...",
    "escrow_snapshot_hash": "blake3:..."
  },
  "summary": {
    "observations": 1200,
    "resolved_existing": 830,
    "promotable_new": 140,
    "abstain_low_evidence": 180,
    "abstain_conflict": 50
  },
  "entities": [
    {
      "state": "RESOLVED_EXISTING",
      "canonical_id": "IC-123abc456def",
      "backbone_rows": ["row-1", "row-9"],
      "attached_rows": [],
      "all_rows": ["row-1", "row-9"],
      "aliases": ["Acme Corp.", "ACME Corporation"],
      "anchors": [{"namespace": "lei", "value": "549300..."}],
      "merge_witnesses": [
        {
          "left_row_id": "row-1",
          "right_row_id": "row-9",
          "pair_score_total": 32,
          "pair_score_by_namespace": {"name": 32}
        }
      ],
      "inheritance": {
        "mode": "single_incumbent_overlap",
        "incumbent_ids": ["IC-123abc456def"]
      },
      "eligible_writeback_aliases": ["ACME Corporation"],
      "escrow": null
    }
  ],
  "abstentions": [
    {
      "state": "ABSTAIN_LOW_EVIDENCE",
      "all_rows": ["row-41", "row-58"],
      "reason": "insufficient_distinct_docs",
      "incumbent_ids": [],
      "escrow": {
        "action": "UPSERT_PENDING",
        "escrow_id": "OE-8f9b7c1d2a3e"
      }
    }
  ],
  "contradictions": [],
  "proposed_registry_patch": {
    "mapping_files": ["org-20260322.json"],
    "new_entity_entries": 140,
    "existing_alias_entries": 27
  },
  "proposed_escrow_patch": {
    "pending_cluster_entries": 52,
    "cannot_link_entries": 11
  }
}
```

### `canon_org_explain.v0`

`canon org explain` emits a proof trace for one query target.

Resolved example:

```json
{
  "version":"canon_org_explain.v0",
  "query":{"row_id":"row-9"},
  "result":{
    "state":"RESOLVED_EXISTING",
    "canonical_id":"IC-123abc456def",
    "escrow_id":null,
    "backbone_rows":["row-1","row-9"],
    "attached_rows":[],
    "inheritance":{"mode":"single_incumbent_overlap","incumbent_ids":["IC-123abc456def"]},
    "witness_chain":[
      {"left_row_id":"row-1","right_row_id":"row-9","operator_ids":["exact_view:core_name"]}
    ]
  }
}
```

Escrow-backed abstention example:

```json
{
  "version":"canon_org_explain.v0",
  "query":{"row_id":"row-41"},
  "result":{
    "state":"ABSTAIN_LOW_EVIDENCE",
    "canonical_id":null,
    "escrow_id":"OE-8f9b7c1d2a3e",
    "backbone_rows":["row-41","row-58"],
    "attached_rows":[],
    "inheritance":{"mode":"no_incumbent_overlap","incumbent_ids":[]},
    "witness_chain":[
      {"left_row_id":"row-41","right_row_id":"row-58","operator_ids":["exact_view:core_name","categorical_equal:industry"]}
    ]
  }
}
```

Required fields:

- `version`
- `query`
- `result`

`result.witness_chain[]` must be derivable from `canon_org_edge.v0` and the
solve artifact without hidden implementation state.

`query` may contain exactly one of:

- `row_id`
- `canonical_id`
- `escrow_id`

### `canon_org_audit.v0`

```json
{
  "version": "canon_org_audit.v0",
  "result": {
    "version": "canon_org_run.v0",
    "content_hash": "blake3:...",
    "strategy_content_hash": "blake3:...",
    "lookup_snapshot_hash": "blake3:...",
    "escrow_snapshot_hash": "blake3:..."
  },
  "suite": { "id": "bdc_org_eval.v1" },
  "summary": {
    "decision": "PROMOTE",
    "hard_gates_passed": true
  },
  "metrics": {
    "gold_pair_f1": 0.982,
    "anchor_consistency": 1.0,
    "anchor_conflicts": 0,
    "holdout_score": 0.975,
    "contradiction_rate": 0.0,
    "perturbation_stability": 0.998,
    "continuity_gain": 0.071,
    "compression_gain": 0.412,
    "registry_churn": 0.006,
    "escrow_reuse_rate": 0.23
  },
  "gate_failures": []
}
```

Required fields:

- `version`
- `result`
- `suite`
- `summary`
- `metrics`
- `gate_failures`

`result.content_hash` is the BLAKE3 hash of the exact UTF-8 bytes of the solve
or run artifact audited. `canon org promote` must verify that this matches the
result artifact it was asked to promote.

`result.version` may be either `canon_org_solve.v0` or `canon_org_run.v0`.

`gold_pair_f1` may be null when a suite relies only on silver signals.

### `canon_org_promote.v0`

```json
{
  "version": "canon_org_promote.v0",
  "result": {
    "version": "canon_org_run.v0",
    "content_hash": "blake3:..."
  },
  "audit": {
    "version": "canon_org_audit.v0",
    "content_hash": "blake3:..."
  },
  "registry": {
    "id": "bdc-issuers",
    "version_before": "2026.03.01",
    "version_after": "2026.03.02",
    "source": "registries/bdc-issuers/",
    "lookup_snapshot_hash_before": "blake3:...",
    "escrow_snapshot_hash_before": "blake3:...",
    "lookup_snapshot_hash_after": "blake3:...",
    "escrow_snapshot_hash_after": "blake3:..."
  },
  "decision": "PROMOTE",
  "writes": {
    "mapping_files": ["org-20260322.json"],
    "new_entity_entries": 140,
    "existing_alias_entries": 27,
    "pending_cluster_entries": 52,
    "cannot_link_entries": 11
  }
}
```

Required fields:

- `version`
- `result`
- `audit`
- `registry`
- `decision`
- `writes`

The promote artifact must describe only writes actually applied to disk under
the provided `--registry`.

Its `registry.*_before` hashes must match the audited result artifact before any
writes are applied.

Its `audit.content_hash` is the BLAKE3 hash of the exact UTF-8 bytes of the
audit artifact used during promotion, and `registry.version_after` must equal
the explicit `--next-version` value.

---

## Registry promotion

### What gets written

Only promoted aliases:

```json
{"input": "Acme Corp.", "canonical_id": "IC-123abc456def", "canonical_type": "org_canon_id", "rule_id": "ORG_PROMOTION:bdc_org_graph.v1"}
{"input": "ACME Corporation", "canonical_id": "IC-123abc456def", "canonical_type": "org_canon_id", "rule_id": "ORG_PROMOTION:bdc_org_graph.v1"}
```

These entries may belong either to:

- a newly promoted `PROMOTABLE_NEW` entity
- a `RESOLVED_EXISTING` entity receiving safe alias expansion

Escrow writes are separate and do not touch the flat lookup mapping files.

### What may be written to escrow sidecars

- `ABSTAIN_LOW_EVIDENCE` pending-cluster hypotheses
- durable `cannot_link_fact` records from `ABSTAIN_CONFLICT`
- trusted-anchor observations and promotion proofs

### What is never written to flat lookup mappings

- abstentions
- contradictions
- raw context fields
- ephemeral graph structure

### Sidecars

Underscore paths may store richer proofs:

```text
registries/bdc-issuers/
  registry.json
  org-20260322.json
  _promotions/
    20260322T153000Z.run.json
  _anchors/
    20260322T153000Z.anchors.jsonl
  _escrow/
    pending.jsonl
    cannot_link.jsonl
```

The lookup path continues to ignore underscore paths.

For `canon org`, these sidecars are not optional decoration. They are the
incumbent-memory layer used during reconciliation and escrow carry-forward.

At minimum, `_anchors/*.jsonl` should let the org resolver answer:

- which trusted anchors have previously been promoted for one `org_canon_id`
- which anchor namespaces are support-only versus trusted in the active profile
- whether a newly solved cluster conflicts with incumbent anchor history

This keeps the fast lookup path flat while still giving `canon org` enough
history to avoid unsafe incumbent inheritance.

At minimum, `_escrow/pending.jsonl` should let the org resolver answer:

- which provisional escrow hypotheses already exist
- which row surfaces, witness pairs, and distinct `doc_id` values support them
- whether a current abstained component should upsert one pending hypothesis or
  create a new one

Suggested pending-cluster record:

```json
{
  "escrow_id":"OE-8f9b7c1d2a3e",
  "profile":"bdc_issuer",
  "doc_ids":["0000000000-25-000001","0000000000-26-000004"],
  "surfaces":["Acme Corp.","ACME Corporation"],
  "anchors":[{"namespace":"lei","value":"549300..."}],
  "witness_pairs":[["row-41","row-58"]],
  "state":"pending"
}
```

Suggested cannot-link record:

```json
{"left_key":"lei:549300AAA","right_key":"lei:549300BBB","reason":"conflicting_trusted_anchor"}
```

---

## Tournament harness

### Suite directory

```text
suites/bdc_org_eval.v1/
  manifest.json
  tune/
  holdout/
  silver_anchors.jsonl
  perturbations.jsonl
  contradictions.yaml
  continuity/
  optional_gold_pairs.jsonl
```

All `source_row_id` values referenced anywhere in the suite must be globally
unique within that suite.

### Suite artifact schemas

#### `manifest.json`

The manifest pins the audit contract for one suite.

Suggested shape:

```json
{
  "suite_id": "bdc_org_eval.v1",
  "profile": "bdc_issuer",
  "thresholds": {
    "max_contradiction_rate": 0.0,
    "min_perturbation_stability": 0.995,
    "non_regression_epsilon": 0.0005
  },
  "budget": {
    "max_runtime_seconds": 120,
    "max_candidate_pairs": 5000000
  }
}
```

`tune/` and `holdout/` contain the normalized-row corpora used for challenger
development and unbiased comparison, respectively. Any fixture intended for a
holdout metric must reference only `source_row_id` values drawn from `holdout/`.

#### `silver_anchors.jsonl`

One line per trusted-anchor observation:

```json
{"source_row_id":"row-1","namespace":"lei","value":"549300..."}
```

Only namespaces marked trusted in the active profile contribute to
`anchor_consistency` and `anchor_conflicts`.

#### `perturbations.jsonl`

One line per harmless-variation set:

```json
{"set_id":"p-001","member_row_ids":["row-a","row-b","row-c"]}
```

All listed rows are expected to preserve one identity outcome under formatting or
surface perturbation.

#### `contradictions.yaml`

One fixture per group that must not collapse:

```yaml
- fixture_id: c-001
  row_ids: ["row-41", "row-58"]
  reason: conflicting_trusted_anchor
```

#### `continuity/*.jsonl`

One labeled pair per line:

```json
{"left_row_id":"row-q1","right_row_id":"row-q2","label":1}
{"left_row_id":"row-q1b","right_row_id":"row-q2b","label":0}
```

These are frozen issuer-continuity proxy labels for adjacent-period slices.

#### `optional_gold_pairs.jsonl`

One gold same/different issuer label per line:

```json
{"left_row_id":"row-1","right_row_id":"row-9","label":1}
{"left_row_id":"row-2","right_row_id":"row-11","label":0}
```

### Metric definitions

All audit metrics are computed against frozen suite artifacts. The tournament may
change the strategy under test, but not the data, formulas, thresholds, or
comparison rules.

#### `gold_pair_f1`

When `optional_gold_pairs.jsonl` is present, treat each labeled pair as a binary
same-entity judgment.

- prediction = 1 iff the pair lands in the same solved component or inherits the
  same `org_canon_id`
- prediction = 0 otherwise, including abstentions into different components

`gold_pair_f1` is the ordinary F1 score on those pair labels.

#### `anchor_consistency`

Treat trusted-anchor fixtures as silver pair labels:

- positive silver pair: two observations share the same trusted anchor value
- negative silver pair: two observations carry conflicting trusted anchor values

A positive silver pair is correct iff both observations land in the same solved
component or inherit the same `org_canon_id`.

A negative silver pair is correct iff the two observations do not land in the
same solved component.

Define:

```text
anchor_consistency =
  (silver_positive_correct + silver_negative_correct) /
  (silver_positive_total + silver_negative_total)
```

#### `perturbation_stability`

Each perturbation set contains multiple harmless surface variants of what should
remain one observation identity.

A perturbation set is stable iff all members end in the same solved component,
the same inherited `org_canon_id`, or the same promoted-new cluster candidate.

Separate abstentions count as unstable even when all members remain unresolved.

Define:

```text
perturbation_stability =
  stable_perturbation_sets / total_perturbation_sets
```

#### `contradiction_rate`

Each contradiction fixture specifies a pair or small set that must not collapse
to one issuer identity.

A contradiction is violated iff all fixture members land in the same solved
component or inherit the same `org_canon_id`.

Define:

```text
contradiction_rate =
  violated_contradiction_fixtures / total_contradiction_fixtures
```

#### `anchor_conflicts`

An anchor conflict occurs when one solved component contains two distinct values
from the same trusted anchor namespace.

For the validated BDC issuer profile, this is evaluated at least for `lei`.

Define:

```text
anchor_conflicts =
  count(solved components with conflicting trusted anchor values)
```

#### `holdout_score`

`holdout_score` is not a free-standing heuristic. It is the geometric mean of
the primary holdout metrics available in the suite.

For the validated BDC issuer profile, the holdout terms are:

- `anchor_consistency_holdout`
- `perturbation_stability_holdout`
- `1 - contradiction_rate_holdout`
- `gold_pair_f1_holdout`, if gold exists on holdout

Each holdout term is computed only from fixtures whose referenced rows live in
the suite's `holdout/` corpus.

Define:

```text
holdout_score = geometric_mean(holdout_terms)
```

This makes the score sensitive to any one safety term collapsing, and prevents a
single high metric from compensating for a dangerous regression elsewhere.

#### `continuity_gain`

Each file under `continuity/` is a frozen adjacent-period issuer-continuity
slice with labeled same-issuer and different-issuer row pairs.

For a given strategy, derive predicted same-issuer judgments from resolved
identity only:

- prediction = 1 iff the two rows inherit the same `org_canon_id` or land in
  the same promoted-new cluster candidate
- prediction = 0 otherwise

Let `continuity_score` be pairwise F1 on those labeled pairs. Then:

```text
continuity_gain =
  continuity_score(candidate) - continuity_score(incumbent)
```

`continuity_gain` is secondary only. It may never override failed hard gates.

#### `compression_gain`

This is a descriptive metric, not a safety metric.

Define it over observations that are not in `ABSTAIN_CONFLICT`:

```text
compression_gain =
  1 - (distinct_resolved_identity_labels / distinct_raw_name_surfaces)
```

Where resolved identity labels are inherited `org_canon_id` values or promoted
new cluster identifiers.

#### `registry_churn`

This measures unnecessary movement against incumbent identity history.

Define:

```text
registry_churn =
  changed_incumbent_assignments / incumbent_comparable_observations
```

Where a changed incumbent assignment is one where the incumbent strategy
inherited one `org_canon_id` from registry-backed evidence and the challenger no
longer inherits that same ID.

#### `escrow_reuse_rate`

This measures how often the accretive escrow layer actually contributes to later
promotion.

Define:

```text
escrow_reuse_rate =
  promoted_new_clusters_with_prior_escrow_id / promotable_new_clusters
```

If `promotable_new_clusters = 0`, define `escrow_reuse_rate = 0`.

### Primary metrics

- `anchor_consistency`
- `holdout_score`
- `gold_pair_f1` when gold exists

### Secondary metrics

- `continuity_gain`
- `compression_gain`
- `abstention_rate`
- `registry_churn`
- `escrow_reuse_rate`

### Hard gates

- zero anchor conflicts
- no holdout regression versus incumbent
- contradiction rate at or below suite threshold
- perturbation stability at or above suite threshold
- budget compliance

#### Hard-gate comparison rule

The suite manifest should define explicit thresholds and a small non-regression
epsilon.

A challenger passes holdout comparison only if:

- `holdout_score(candidate) + epsilon >= holdout_score(incumbent)`
- each available primary holdout metric is also non-regressing within `epsilon`

This prevents the geometric mean from hiding a meaningful drop in one primary
metric.

### Search policy

Use the same champion/challenger pattern as the parser tournament:

- one incumbent strategy
- three challengers per round
- bounded rounds
- one promotion decision at a time

Suggested mutation types:

- conservative
- blocker_targeted
- anchor_targeted
- contradiction_targeted
- compression_targeted
- domain_context_targeted

Only allow bounded code-patch mutation after strategy-space plateau.

---

## Domain profiles

### Validated v1 profile: BDC issuer

Typical surfaces:

- `portfolio_company`
- `industry`
- `investment_type`
- `interest_rate`
- `maturity_date`
- `par_amount`

Notes:

- instrument fields are weak support for issuer identity, not decisive identity
  on their own
- equity and debt in the same company should usually support the same issuer,
  not force separate issuer IDs

### Future profiles

Additional organization-identity profiles may be added later without changing
the `canon org` CLI or artifact schema, but this plan does not claim that those
profiles are solved or even well-specified yet.

---

## Implementation shape

This section maps the plan onto the current `canon` crate as it exists today.
It is intentionally narrower than the abstract architecture above.

### Current codebase constraints

The present crate is organized around one exact-match lookup pipeline:

- `src/cli.rs` defines one flat lookup CLI plus the `registry` namespace
- `src/lib.rs` dispatches either the lookup pipeline or `registry` subcommands
- `src/input.rs` parses one selected column into deduplicated `InputValues`
- `src/lookup.rs` resolves those deduplicated values against the SQLite index
- `src/output/json.rs` and `src/output/csv.rs` emit only the lookup artifacts
- `src/registry.rs` loads flat mapping files and builds the lookup SQLite index

That has direct implications for `canon org`:

- do not try to extend `InputValues` for org identity work
- do not try to extend `ResolveResult` or `CanonOutput` for org artifacts
- do not overload the current SQLite lookup index with escrow or anchor sidecars
- do not force org output through the current `output/json.rs` or `output/csv.rs`

### Non-goals for the first implementation

To keep the first cut tractable:

- do not add generic graph infrastructure beyond the staged solver in this plan
- do not add `petgraph` in v1 unless the staged solver demonstrably needs it
- do not make `canon org` share the lookup code path after parse
- do not update `operator.json` until at least `run`, `audit`, and `promote`
  artifacts are stable

### Fit to current crate boundaries

The right implementation cut is:

- keep the existing lookup path in `src/lib.rs::run_pipeline()` unchanged
- add a new sibling `org` namespace under `CanonCommand`
- add a new `src/org/` module tree that owns org-specific types, parsing,
  incumbent memory loading, solver logic, and output artifacts
- reuse the existing refusal envelope and witness ledger conventions, but not the
  lookup-specific result structs

Concretely:

- the current top-level `canon <INPUT> --registry ... --column ...` path remains
  exact-match lookup only
- `canon org ...` becomes a separate subcommand family beside `canon registry`
- org result artifacts are separate Rust structs and separate `version` strings
- org sidecar loading is separate from `registry::load_registry()`

### Files to modify
| File | Change |
|------|--------|
| `src/cli.rs` | Add `CanonCommand::Org` and org subcommand argument structs |
| `src/lib.rs` | Add dispatch only; keep lookup-only wire types untouched |
| `src/refusal.rs` | Add org-specific refusal helpers and actionable next commands |
| `src/lib.rs` | Extend `RefusalCode` serialization with `E_ORG_*` variants |
| `Cargo.toml` | Add `serde_yaml`; do not add heavier graph deps initially |
| `operator.json` | Add `canon org` surface only after artifact contracts are implemented |

### New files

```text
src/org/
  mod.rs
  types.rs
  strategy.rs
  projection.rs
  incumbent.rs
  block.rs
  edge.rs
  solve.rs
  audit.rs
  promote.rs
  explain.rs
  output.rs
```

### Ownership by module

- `src/org/types.rs`
  owns `canon_org_block.v0`, `canon_org_edge.v0`, `canon_org_solve.v0`,
  `canon_org_run.v0`, `canon_org_audit.v0`, `canon_org_promote.v0`,
  `canon_org_explain.v0`, and the internal projection structs
- `src/org/strategy.rs`
  owns YAML parsing, schema validation, and normalized in-memory strategy types
- `src/org/projection.rs`
  owns row-preserving CSV/JSONL reads for org identity input, including
  `source_row_id`, `doc_id`, side-field parsing, and max-row/max-byte checks
- `src/org/incumbent.rs`
  owns flat registry alias loading, `_anchors/`, `_escrow/`, snapshot hashing,
  and reconciliation memory
- `src/org/block.rs`
  owns candidate neighborhood generation and `canon_org_block.v0`
- `src/org/edge.rs`
  owns evidence generation and `canon_org_edge.v0`
- `src/org/solve.rs`
  owns seed/backbone/attachment solving, inheritance, and escrow actions
- `src/org/audit.rs`
  owns suite loading, metric computation, and hard-gate decisions
- `src/org/promote.rs`
  owns side-effectful write-back, version bump enforcement, and
  `canon_org_promote.v0`
- `src/org/explain.rs`
  owns proof-trace extraction from solve/run artifacts
- `src/org/output.rs`
  owns JSON and summary emission for org artifacts

### What can actually be reused

Safe reuse:

- refusal envelope shape
- witness ledger append semantics
- BLAKE3 hashing helpers already used elsewhere
- registry metadata conventions from `registry.json`

Do not reuse directly:

- `src/input.rs`
  because it deduplicates one selected column into `HashMap<String, ()>` and
  discards row identity and side fields
- `src/lookup.rs`
  because org identity is not exact-match lookup
- `src/output/json.rs` / `src/output/csv.rs`
  because their schemas and redaction rules are lookup-specific
- `registry::load_registry()`
  because it returns only flat lookup metadata plus SQLite db path, not org
  incumbent memory or snapshot hashes

### Specific codebase deltas

The implementation will need these explicit deltas to the current code:

1. CLI shape

- add `CanonCommand::Org(OrgCommand)` beside `Registry`
- keep the existing flat lookup CLI untouched
- keep `canon org` fully subcommand-based to avoid fighting the current
  `required_unless_present_any` lookup args

2. Shared refusal codes

- extend `RefusalCode` with the `E_ORG_*` variants defined in this plan
- keep the existing `Refusal` envelope struct unchanged
- add org helper constructors in `src/refusal.rs`

3. Shared type boundaries

- leave `CanonOutput`, `ResolveResult`, `InputValues`, and `Registry` as
  lookup-path types
- add separate org result structs under `src/org/types.rs`
- do not centralize org artifact types in `src/lib.rs`

4. Registry loading

- keep `src/registry.rs` responsible for flat lookup registries and SQLite index
- add `src/org/incumbent.rs` for org-specific registry memory:
  - flat alias mappings
  - `_anchors/*.jsonl`
  - `_escrow/pending.jsonl`
  - `_escrow/cannot_link.jsonl`
  - deterministic snapshot hashes

5. Output and redaction

- keep current lookup JSON/CSV emitters unchanged
- add org-specific JSON and summary emitters under `src/org/output.rs`
- if shared identifier encoding is useful, extract only that helper into a
  neutral common location later; do not couple org output to lookup output

6. Witnessing

- reuse the existing witness ledger format and ambient append behavior
- factor small helper(s) out of `src/lib.rs` only if org commands would
  otherwise duplicate witness-record assembly verbatim

### Suggested build order

1. CLI scaffolding and `RefusalCode` extension
2. org artifact/result types
3. strategy parser with `serde_yaml`
4. projection layer
5. incumbent-memory loader and snapshot hashing
6. block engine
7. evidence edge engine
8. staged solver
9. audit artifact and suite reader
10. promotion/write-back with `--next-version`
11. explain and summary emitters
12. `operator.json` update and end-to-end tests

---

## Why this gets humans mostly out of the loop

The key move is not “automate all ambiguity.”

The key move is:

- auto-promote only safe clusters
- escrow edge cases cheaply instead of dropping them
- let later observations add evidence
- keep a frozen silver-plus-gold harness so promotion is automatic

That turns manual alias-sheet maintenance into:

- rare primitive expansion
- rare suite curation
- normal automatic compounding through promotion and escrow carry-forward

---

## Final rule

If a promoted merge or escrow update cannot be explained as a short sequence of
explicit evidence lines and hard-gate passes, it does not ship.
