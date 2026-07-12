# Canon Identity Architecture

> Current architecture note. Canon is an identity compiler: messy local evidence
> is converted into reviewed, versioned registry knowledge; production replay is
> exact lookup against that registry. Historical plans remain useful context only
> when they agree with this boundary.

---

## Doctrine

`canon` should be understood as three cooperating layers:

1. **Lookup kernel**
   - Command family: `canon <INPUT> --registry <DIR> --column <COLUMN>`
   - Contract: exact byte match after ASCII-trim against a local versioned
     registry.
   - Output: a `canon.v0` mapping artifact or row-preserving CSV.
   - Invariant: no fuzzy matching, no clustering, no live enrichment, no
     heuristic guessing at resolution time.

2. **Evidence workbenches**
   - Command family: `canon entity`. Cluster mode handles one profiled corpus;
     link mode handles two row sets through the same artifact-backed workbench.
     No long-lived compatibility alias is promised for superseded workbench
     namespaces.
   - Contract: run a bounded deterministic strategy against frozen local inputs,
     emit candidates/evidence/solve/audit/review artifacts, then promote accepted
     registry updates.
   - Output: registry mapping entries, trusted-anchor sidecars, escrow sidecars,
     and proof artifacts.
   - Invariant: sophisticated evidence is allowed only before promotion. Once
     promoted, ordinary `canon` runs still resolve through exact registry lookup.

3. **Distribution and extension surfaces**
   - Command families and workflows: `canon registry export`, `canon package`,
     project-mode dispatch paths, temporal snapshot tools, provider
     materializers, and out-of-tree extensions.
   - Contract: package, ship, project, audit, or specialize registry knowledge
     without changing the lookup kernel.
   - Output: dbt seeds, SQLite search indexes, signed packages, project locks,
     temporal comparison artifacts, provider snapshots, and extension-owned
     profiles or adapters.
   - Invariant: industry ontology, provider knowledge, and domain-specific
     decision policy live outside the core defaults unless explicitly packaged
     and audited by the operator.

This is the central distinction that reconciles the core plan with
`canon entity`: **core `canon` is not a probabilistic entity-resolution runtime,
but `canon` can host deterministic resolution workbenches that compile messy
observations into reviewed versioned registries.**

The lifecycle is:

```text
messy evidence -> deterministic artifacts -> audit/review -> versioned registry -> exact replay
```

### Strategy Doctrine

`canon strategy` is canonical procedural knowledge, not exact lookup. A
strategy is selected under a typed doctrine, executed locally under a bounded
policy, audited with deterministic fixtures, and only then allowed to promote
procedural or registry-mutating consequences.

The canonical typed kinds are:

| Kind | Selection key | Allowed inputs | Declared outputs | Compatibility | Promotion target |
|------|---------------|----------------|------------------|---------------|------------------|
| `identity-evidence` | profile id + skill hash | profiled observations | evidence bundle + registry-knowledge proposal | profile-scoped | audited, review-gated registry knowledge |
| `record-linkage` | linkage map id + skill hash | two-tape records | linkage bundle + registry-knowledge proposal | field-map-scoped | audited, review-gated registry knowledge |
| `schema-transform` | schema fingerprint + skill hash | schema/profile artifact | frozen script pointer | exact / compatible / partial / unresolved schema tiers | versioned strategy registry champion |
| `task-transform` | exact task key + skill hash | exact task request | frozen script pointer | exact only | versioned strategy registry champion |

Doctrine rules:

1. Cross-kind selection is impossible. A strategy key, input contract,
   compatibility relation, and promotion target must all agree on one typed
   kind.
2. Exact lookup remains exact lookup. Strategy selection or execution is never
   part of `canon <INPUT> --registry ... --column ...` and is never part of exact lookup.
3. Transform kinds select procedural champions; they do not create same-entity
   claims by themselves.
4. Evidence and linkage kinds may manufacture audited proposals for registry
   updates, but only through the workbench path and its audit/review gates.
5. Mixed schema/task/profile/linkage contracts are migration errors, not
   operator discretion.

Current implementation note: the existing runtime surfaces in `canon strategy`
are schema-transform and task-transform registry champions. Identity-evidence
and record-linkage are the canonical doctrine for future typed procedural
strategies and must remain outside the exact lookup kernel.

### Cluster Mode And Link Mode

`canon entity` has two public shapes:

- **Cluster mode** (`canon entity run` and its stage commands) groups profiled
  observations inside one corpus. It can produce solved clusters, review
  queues/inboxes, escrow, and promotion proposals.
- **Link mode** (`canon entity link <REFERENCE> <TARGET>`) aligns two row sets
  through the same typed request and artifact path as project mode. It is a
  cross-source linkage workflow, not a public `edge` alias and not a shortcut
  around evidence, audit, or review.

Both modes may use support evidence, anti-merge evidence, and relation hints.
Relationship or hierarchy evidence must remain a relation hint unless a separate
profile-approved equality signal supports a same-entity decision.

---

## Why This Is Not A Contradiction

The original `PLAN_CANON.md` non-goals protect the lookup kernel. They remain
correct:

- core lookup is not a fuzzy matcher
- core lookup is not a master data management system
- core lookup is not a generic record linker
- core lookup is not an address parser or geocoder
- core lookup is not a data-cleansing engine

`canon entity` does not weaken those constraints. It runs outside the lookup
kernel. It may block candidate pairs, score deterministic evidence, solve
clusters, abstain on low-confidence or contradictory cases, and require audit
before writing registry updates. After that write, the durable product is still a
flat versioned registry that the lookup kernel can resolve exactly.

So the precise product claim is:

> `canon` is not an MDM platform. It is a registry-centered identity compiler:
> exact lookup for production replay, plus deterministic local workbenches for
> creating, auditing, packaging, and projecting the registries that lookup
> depends on.

---

## Capability Matrix

| Capability | Command family | Status | Identity object | Matching mode | Persistent output |
|------------|----------------|--------|-----------------|---------------|-------------------|
| Exact identifier lookup | `canon <INPUT>` | Implemented | Input value to canonical ID | Exact registry lookup | None; emits mapping artifact |
| Provider-backed registry maintenance | `canon registry build/diff/audit/lint` | Implemented | Registry entries | Provider-backed materialization, diff, audit, lint | Versioned registry files |
| Self-authored registry maintenance | `canon registry next-id/add-entry/mint/default-id-scheme` | Implemented | Operator-chosen canonical IDs and aliases | Exact alias authoring under a local ID convention; not a resolution workbench | Flat mapping entries plus `registry.json` metadata |
| Strategy registry | `canon strategy` | Implemented | Schema and skill to frozen script | Deterministic script selection | Versioned strategy registry |
| Profiled entity workbench | `canon entity` | Current generic workbench namespace | Profile-scoped observations such as legal entities, funds, people, brands, properties, assets, or domain-extension observations | Native Rust normalization, bounded blocking, typed support evidence, anti-merge evidence, relation hints, deterministic solver, abstention | Alias entries, anchor sidecars, cannot-link sidecars, escrow sidecars, proofs |
| Organization identity legacy plan | Legacy org plan only | Superseded by `canon entity`; no compatibility alias promised | Organization observation to `org_canon_id` in legacy BDC/issuer-like profiles | Blocking, typed evidence, deterministic solver, abstention | Alias entries, anchor sidecars, escrow sidecars, proofs |
| Cross-source linkage | `canon entity link` | Implemented under the generic entity workbench namespace | Record in reference rows to record in target rows | Structural evidence under an explicit link strategy; deterministic abstention on unmatched/ambiguous records; shared entity artifacts plus decision projection | `canon_entity_link.v0` with `canon_entity_link_decisions.v0` and optional flat cross-reference registry entries |
| Property/address identity | Future workbench | Planned | Property observation to property canonical ID | Address/geospatial/name evidence under deterministic strategy | Property registry entries and proofs |
| Fuzzy suggestions | Future assistive workflow only | Deferred | Unresolved value to suggested candidate | Probabilistic candidate generation, never auto-accepted | Human-approved registry entries only |

## Extension Boundary

Extensions are allowed to add profiles, adapters, strategy packages, provider
materializers, review policy, schema projections, and domain-specific
documentation. They are not allowed to smuggle domain knowledge into the core
lookup defaults or change the runtime match rule.

Core Canon may define the neutral contract for:

- how observations become prepared surfaces
- how candidates, evidence, solves, reviews, packages, project locks, temporal
  snapshots, and exports are represented
- how refusals and witness records behave

Extensions own:

- ontology and vocabulary choices
- provider-specific semantics and credentials
- domain thresholds and review policy
- adapter-specific field mappings
- commercial or private registry content

That separation lets Canon run standalone on local registries while still
supporting richer packaged deployments.

---

## Workbench Rules

Any new resolution workbench must satisfy these rules before it can be treated
as part of `canon` rather than an ad hoc matcher:

1. **Local and deterministic**
   - Same observations, same strategy, same registry, and same sidecars produce
     byte-stable artifacts.
   - No live network calls or runtime LLM decisions in the production execution
     path.

2. **Evidence before decisions**
   - Blocking, scoring, and solving must emit inspectable artifacts.
   - Every merge, abstention, contradiction, and promotion must have a
     machine-readable reason trail.

3. **Abstention is a feature**
   - Ambiguous cases must enter escrow or review, not be guessed.
   - Conflicting trusted anchors must create contradiction/cannot-link evidence.

4. **Audit gates promotion**
   - Registry mutation requires explicit version bumping and snapshot checks.
   - Alias and anchor promotion requires a matching passing audit artifact.

5. **Registry remains the durable asset**
   - Workbench-specific evidence can live in sidecars and proofs.
   - Production lookup remains `input -> canonical_id` through versioned registry
     files and derived indexes.

Self-authored registry maintenance is deliberately outside these workbench
rules. `canon registry mint` and `add-entry` do not infer identity from messy
observations; they record an operator's already-accepted canonical ID and exact
aliases as flat mapping entries. `default-id-scheme` is metadata for consistent
local ID allocation, not a new matching mode.

---

## Documentation Map

- `docs/PLAN_CANON.md`: current source of truth for the core lookup kernel, registry
  substrate, refusal semantics, exact-match invariants, and shared release
  contract.
- `docs/PLAN_ENTITY_WORKBENCH.md`: current workbench plan for the generic
  workbench to `canon entity`, adding native Rust namekit primitives,
  first-class anti-merge evidence, profile package support, performance
  hardening, and smoother operator ergonomics.
- `docs/PLAN_ORG_IDENTITY_TOURNAMENT.md`: historical organization-identity plan
  retained for implementation archaeology. Do not present it as the active
  public namespace or current architecture; active docs should use
  `canon entity`.
- `docs/PLAN_STRUCTURAL_RESOLUTION.md`: historical/internal source for the
  preserved cross-tape decision engine now surfaced through `canon entity link`;
  it should not be read as current core lookup behavior or as a separate public
  namespace.
- `docs/PLAN_BDC_ENTITY_REGISTRATION.md`: historical/domain bootstrap plan that
  fit under the legacy organization workbench model. Domain-specific knowledge
  belongs in packages, profiles, registries, or extensions, not in Canon core
  defaults.

When these documents appear to disagree, use this hierarchy:

1. Core lookup behavior follows `PLAN_CANON.md`.
2. Workbench behavior follows its workbench-specific plan.
3. This architecture note defines the boundary between them.
