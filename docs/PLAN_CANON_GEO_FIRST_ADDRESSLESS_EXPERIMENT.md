# PLAN_CANON_GEO_FIRST_ADDRESSLESS_EXPERIMENT

> **Status:** next-action implementation note.  
> **Date:** 2026-09-16.  
> **Target:** first work session 2026-09-17.  
> **Related:** `PLAN_CANON_GEO.md`, `PLAN_CANON_GEO_PHYSICAL_ASSET_RESOLUTION.md`.  
> **Goal:** make the smallest defensible change to current Canon Geo that permits one blind, addressless physical-asset resolution attempt.

## 1. Decision

Do **not** redesign Canon Geo before attempting the new use case.

The current implementation already contains most of the difficult machinery:

- bounded parcel/building candidate universes;
- parcel and building selection profiles;
- exact residual enumeration;
- backbone reporting;
- soft preferences that rank but do not force residuals;
- explicit abstention behavior;
- typed evidence admission through rho contracts;
- separation of logically sound evidence from empirical/soft evidence;
- protocol-neutral discovery/acquisition handoffs;
- source/release pins and provenance;
- collateral-completeness observations;
- client property attribute channels including building size, year built, and property type.

The immediate gap is narrower:

> **The existing six-field client profile expects address, geocode, or geometry to act as a driver/membership channel. It does not yet permit a bounded geography plus descriptive attributes to seed and collapse the candidate universe on their own.**

Therefore the first task is to add one experimental descriptive-asset profile and the minimum evidence adapters required to exercise it.

## 2. What current code already proves

### Composition kernel

`src/geo/composition.rs` already implements a bounded parcel/building composition kernel that enumerates hard-feasible models exactly within budget, reports the exact residual/backbone, and uses soft preferences only to rank unresolved alternatives.

This is sufficient for the first experiment. Do not build a new solver.

### Existing six-field property profile

The current client property contract contains:

```text
Geocode
Address
Geometry
BuildingSize
YearBuilt
PropertyType
```

All may be absent. Building size, year built, and property type already have attribute-rejector / assemblage-constraint roles.

However, current decision bands expect at least one reliable geometry/address driver for an exact-residual/soft-ranked result. With no address, geocode, or geometry, the profile correctly tends toward unsupported/waiting-for-input.

### Evidence admission

`src/geo/evidence.rs` already prevents raw observations from becoming solver constraints automatically. Rho contracts distinguish logically sound constraints from empirical high-coverage observations and support declared admission, corroboration requirements, supported-member thresholds, soft weighting, and diagnostic-only behavior.

Preserve this architecture.

### Candidate acquisition

`src/geo/discovery.rs` already defines protocol-neutral bounded discovery/acquisition contracts. External executors/catalog/warehouse/source operators acquire the data; Canon consumes pinned artifacts.

Do not put scraping/network acquisition into the solver.

### Collateral completeness

`src/geo/collateral_schedule.rs` already supports source-authored complete member sets becoming typed `AllOf` + `ExactCardinality` evidence. This can later be useful for SEC collateral schedules, county-recorded parcel sets, planning applications, or data-center parcel assemblages.

## 3. First code change

Add one experimental profile, working name:

```text
descriptive_asset_resolution_v0
```

Naming/versioning should be reconciled with existing Canon conventions during implementation; do not treat this working name as frozen public protocol.

The key semantic change is the candidate-universe rule:

> **A declared bounded geography may itself seed the candidate universe. Descriptive evidence may eliminate or rank members without requiring address, geocode, or supplied geometry as the initial driver.**

This is the smallest important change.

Do not weaken the existing `client_property_six_field` contract. Add a separate profile.

## 4. Minimum channels for experiment 1

Do not implement the full sidecar schema tomorrow.

Start with four descriptive channels:

```text
PropertyName
PropertyType
UnitCount OR BuildingSize
YearBuilt
```

If the selected property lacks one of these, absence must remain explicit and non-fabricated.

Potential fifth channel, only if needed to collapse the first residual:

```text
Owner / LegalEntity
```

### Semantics

`PropertyType`, `UnitCount/BuildingSize`, and `YearBuilt` should compile into deterministic or explicitly banded candidate constraints where source semantics permit.

`PropertyName` should initially be treated conservatively as candidate/soft evidence unless a source contract provides stronger alias semantics.

Do not make fuzzy name similarity a hard identity rule.

## 5. First blind NXRT experiment

Choose **one NXRT multifamily property** for which:

- the public SEC disclosure supplies a property name and bounded geography;
- at least two useful descriptive attributes are available;
- the true street address and parcel/building composition can be independently established for evaluation;
- the geography has obtainable public parcel/building/assessor evidence.

### Freeze the truth

Before the run, separately record the known ground truth:

```text
property
street address
parcel id(s)
building id(s) / footprint(s)
```

Do not expose those values to the resolver inputs.

### Resolver input

Example shape:

```text
property_name: <SEC disclosed name>
city_or_market: <SEC disclosed geography>
state: <state>
property_type: multifamily
units: <if disclosed>
year_built: <if disclosed>
as_of: <filing date>
```

No address. No known geocode. No known target geometry.

### Candidate universe

Acquire a bounded regional inventory containing, as available:

- parcels;
- building footprints;
- assessor/property-use attributes;
- unit/size/year-built attributes;
- source identifiers and release pins.

The first experiment may use manually prepared/pinned local artifacts. Do not block on automated nationwide acquisition.

### Expected behavior

Illustrative only:

```text
starting parcel/property candidates  12,483
multifamily-compatible                  211
unit-count-compatible                    17
year-built-compatible                     6
name-supported                            2

RESULT
2-candidate exact residual
```

A two-candidate residual is a successful first run if it is exact and the true property remains reachable.

Then ask:

> **What additional evidence separates the remaining candidates?**

If owner/legal-entity evidence is the answer, add that evidence plane and rerun rather than adding an arbitrary heuristic.

A later result may become:

```text
2 candidates
→ owner/legal-entity evidence
→ 1 candidate
→ FORCED
```

## 6. Success criteria for tomorrow

The experiment succeeds if all of the following are true:

1. The true asset is present in the bounded candidate universe.
2. Descriptive evidence reduces the universe materially without address/geocode/target geometry.
3. Canon returns an exact residual or forced result rather than an opaque score.
4. No false candidate is promoted merely because it ranks highest.
5. Missing evidence remains explicit.
6. The run preserves source/release/evidence provenance.
7. We can identify the next evidence needed if the result remains ambiguous.

**It is not necessary to force the correct answer on the first run.**

A clean residual is more valuable than a guessed address.

## 7. Metrics to capture immediately

Even for one property, record:

```text
initial candidate count
candidate reach: true/false
candidate count after each admitted constraint
hard constraints admitted
soft/diagnostic evidence used
final residual size
forced identity: yes/no
forced identity correct: yes/no
withheld truth reachable: yes/no
acquisition gaps
next-evidence recommendation
runtime / solver budget
```

This becomes the seed of the eventual NXRT blind benchmark.

## 8. What not to build tomorrow

Do not start with:

- nationwide parcel acquisition;
- all 36 NXRT properties;
- a general data-center ontology;
- PowerMW semantics;
- full owner graph traversal;
- a new solver;
- LLM/Jev integration;
- Evidence Machine orchestration;
- a production API;
- a universal `geo_entity_claim` schema.

Prove the semantic extension first.

## 9. Data-center follow-on

Once one NXRT property works, use the same profile concept on one public data-center project.

For the first data-center experiment, prioritize evidence that maps naturally onto physical composition:

```text
county / municipality
project/developer name
acreage
land use / zoning
owner / legal entity
planning or permit identifier
building count / square footage
substation or infrastructure proximity
```

Defer `PowerMW` as a hard physical constraint until a defensible evidence contract exists; it is initially more indirect than acreage, parcel ownership, or planning evidence.

The physical result may be a **parcel assemblage**, not one parcel or one address.

## 10. Jev/model follow-on

After deterministic descriptive channels work, evaluate Jev or another typed judgment model for bounded soft questions such as:

```text
Are these property/project names aliases?
Is this assessor use category compatible with the disclosed property type?
Is this owner LLC plausibly linked to the disclosed sponsor/operator?
```

Model output should enter through the existing evidence-admission architecture as candidate/soft evidence unless a separately governed contract permits more.

Do not let model confidence mint physical identity.

## 11. Implementation posture

The intended change is:

```text
CURRENT CANON GEO
bounded universe
+ exact composition solver
+ evidence admission
+ residual/backbone
+ abstention

        PLUS

small descriptive-asset profile
+ descriptive evidence adapters
+ geography-as-candidate-seed rule

        EQUALS

first addressless physical-asset resolution experiment
```

The difficult engine already exists. Tomorrow's objective is to give it a new, disciplined entrance into the candidate universe.

## 12. First principle

> **Do not ask Canon Geo to guess the address. Ask it to prove how far the available evidence collapses the physical candidate universe.**

If one candidate survives under admissible evidence, physical identity is established under the declared profile. If several survive, preserve the residual and ask for the evidence that can separate them.