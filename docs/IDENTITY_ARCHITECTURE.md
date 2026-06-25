# Canon Identity Architecture

> Canon has one registry substrate and multiple identity workflows. The core
> lookup path stays exact. Resolution workbenches manufacture new registry
> knowledge through deterministic, auditable evidence pipelines.

---

## Doctrine

`canon` should be understood as two layers:

1. **Lookup kernel**
   - Command family: `canon <INPUT> --registry <DIR> --column <COLUMN>`
   - Contract: exact byte match after ASCII-trim against a local versioned
     registry.
   - Output: a `canon.v0` mapping artifact or row-preserving CSV.
   - Invariant: no fuzzy matching, no clustering, no live enrichment, no
     heuristic guessing at resolution time.

2. **Resolution workbenches**
   - Command families: `canon entity` and `canon resolve` today. `canon entity`
     is the direct replacement for the generic observation -> evidence -> solve
     -> audit -> promote workbench formerly embodied by the legacy org
     plan; no long-lived compatibility alias is promised.
   - Contract: run a bounded deterministic strategy against frozen local inputs,
     emit candidates/evidence/solve/audit/review artifacts, then promote accepted
     registry updates.
   - Output: registry mapping entries, trusted-anchor sidecars, escrow sidecars,
     and proof artifacts.
   - Invariant: sophisticated evidence is allowed only before promotion. Once
     promoted, ordinary `canon` runs still resolve through exact registry lookup.

This is the central distinction that reconciles the core plan with
`canon entity`: **core `canon` is not a generic entity-resolution engine, but
`canon` can host deterministic resolution workbenches that compile messy
observations into versioned registries.**

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

> `canon` is not an MDM platform. It is a canonical identity workbench: exact
> lookup for production resolution, plus deterministic domain workbenches for
> creating and auditing the registries that lookup depends on.

---

## Capability Matrix

| Capability | Command family | Status | Identity object | Matching mode | Persistent output |
|------------|----------------|--------|-----------------|---------------|-------------------|
| Exact identifier lookup | `canon <INPUT>` | Implemented | Input value to canonical ID | Exact registry lookup | None; emits mapping artifact |
| Provider-backed registry maintenance | `canon registry build/diff/audit/lint` | Implemented | Registry entries | Provider-backed materialization, diff, audit, lint | Versioned registry files |
| Self-authored registry maintenance | `canon registry next-id/add-entry/mint/default-id-scheme` | Implemented | Operator-chosen canonical IDs and aliases | Exact alias authoring under a local ID convention; not a resolution workbench | Flat mapping entries plus `registry.json` metadata |
| Strategy registry | `canon strategy` | Implemented | Schema and skill to frozen script | Deterministic script selection | Versioned strategy registry |
| Profiled entity workbench | `canon entity` | Current generic workbench namespace | Profile-scoped observations such as tenant labels, legal entities, funds, people, brands, or properties | Native Rust normalization, bounded blocking, typed merge evidence, anti-merge evidence, deterministic solver, abstention | Alias entries, anchor sidecars, cannot-link sidecars, escrow sidecars, proofs |
| Organization identity legacy plan | Legacy org plan only | Superseded by `canon entity`; no compatibility alias promised | Organization observation to `org_canon_id` in legacy BDC/issuer-like profiles | Blocking, typed evidence, deterministic solver, abstention | Alias entries, anchor sidecars, escrow sidecars, proofs |
| Cross-tape structural resolution | `canon resolve` | Implemented v0 | Record in reference tape to record in target tape | Structural evidence under an explicit two-tape strategy; deterministic abstention on unmatched/ambiguous records | `canon_resolve.v0` evidence and optional flat cross-reference registry entries |
| Property/address identity | Future workbench | Planned | Property observation to property canonical ID | Address/geospatial/name evidence under deterministic strategy | Property registry entries and proofs |
| Fuzzy suggestions | Future assistive workflow only | Deferred | Unresolved value to suggested candidate | Probabilistic candidate generation, never auto-accepted | Human-approved registry entries only |

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

- `docs/PLAN_CANON.md`: source of truth for the core lookup kernel, registry
  substrate, refusal semantics, exact-match invariants, and shared release
  contract.
- `docs/PLAN_ENTITY_WORKBENCH.md`: direct-replacement plan for renaming the generic
  workbench to `canon entity`, adding native Rust namekit primitives,
  first-class anti-merge evidence, CMBS tenant-label support, performance
  hardening, and smoother operator ergonomics.
- `docs/PLAN_ORG_IDENTITY_TOURNAMENT.md`: legacy organization-identity plan
  retained for historical implementation context; active public workbench docs
  should use `canon entity`.
- `docs/PLAN_STRUCTURAL_RESOLUTION.md`: source of truth for the implemented
  v0 `canon resolve` cross-tape structural record workbench; it should not be
  read as current core lookup behavior.
- `docs/PLAN_BDC_ENTITY_REGISTRATION.md`: domain bootstrap plan that fit under
  the legacy organization workbench model and should migrate with that
  workbench to `canon entity`.

When these documents appear to disagree, use this hierarchy:

1. Core lookup behavior follows `PLAN_CANON.md`.
2. Workbench behavior follows its workbench-specific plan.
3. This architecture note defines the boundary between them.
