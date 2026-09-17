# PLAN_CANON_GEO_PHYSICAL_ASSET_RESOLUTION — Resolve Incomplete Asset Claims to Physical Identity

> **Status:** sidecar proposal / candidate near-term proof.  
> **Date:** 2026-09-16.  
> **Parent:** `PLAN_CANON_GEO.md`.  
> **Priority note:** this may be a better primary proving case for Canon Geo than address-first resolution. Address-first remains valid and is not removed.  
> **Do not silently redefine Canon core:** Canon runtime lookup remains exact registry replay; this is a Canon Geo build-time resolution workload.

## 1. Thesis

Canon Geo should not be framed only as:

```text
known address → parcel(s) → building(s)
```

A more general and potentially more valuable problem is:

```text
INCOMPLETE DESCRIPTION OF A REAL-WORLD ASSET
                ↓
BOUNDED GEOGRAPHIC UNIVERSE
                ↓
CANDIDATE PHYSICAL ASSETS
                ↓
EVIDENCE + CONSTRAINTS
                ↓
PHYSICAL IDENTITY / COLLATERAL COMPOSITION
```

An address is one strong evidence class, not a required starting key.

The question Canon Geo should be able to answer is:

> **What physical thing does this observation refer to?**

The answer may be forced, supported, ambiguous, conflicted, unreachable, or otherwise explicitly unresolved. Canon Geo must not turn candidate ranking into identity truth.

This generalization is especially relevant to two near-term public-data workloads:

1. **public REIT collateral** — a named property with city/market, type, size/units and other descriptors but no trusted address supplied to the resolver;
2. **data-center / infrastructure projects** — a project/developer/county/acreage/power description where the source often never supplies a conventional street address at all.

## 2. Proving case A: public REIT collateral

NexPoint Residential Trust (NXRT) is the motivating example. Public filings/supplements can provide observations such as property/community name, city/state or market, property type, unit count, acquisition information, operating attributes, mortgage/debt relationships, ownership/subsidiary context, and other characteristics.

The address should be withheld from the resolver.

Example:

```text
name: Arbors of Brentwood
city_or_market: Nashville / Brentwood
state: TN
property_type: multifamily
units: <if disclosed>
year_built: <if disclosed>
acquisition_date: <if disclosed>
acreage: <if disclosed>
owner_or_subsidiary: <if disclosed>
as_of: <filing/evidence date>
```

Canon Geo should attempt to establish which parcel/building composition in the bounded geography this claim refers to.

## 3. Proving case B: data-center / infrastructure site resolution

Data centers may be an even stronger expression of the generalized primitive because the original evidence frequently does **not** contain a usable street address.

A public observation may look like:

```text
project_name: Project Beacon
developer_or_operator: <company>
county: <county>
state: <state>
municipality: <if known>
site_acres: ~450
planned_power_mw: ~600
planned_buildings: 4
near: <road / substation / landmark>
owner_llc: <if discovered>
project_stage: proposed / permitted / construction
as_of: <evidence date>
```

Other evidence surfaces may contribute project aliases, developer/operator names, parcel-owner LLCs, planning cases, zoning applications, tax incentive agreements, building permits, acreage, building square footage, electrical load, utility territory, substation proximity, interconnection requests, road frontage, water/sewer applications, land-development records, government agendas/minutes, filings, press releases, parcels, buildings, OSM and Overture.

Canon Geo should resolve the **physical site**, which may be an assemblage of many parcels and multiple buildings rather than one postal address.

Illustrative collapse:

```text
75,000 county parcels
        ↓
eligible industrial / data-center use
        ↓
parcel assemblages compatible with ~450 acres
        ↓
owner / developer LLC relationship
        ↓
near named substation / utility evidence
        ↓
planning or permit case compatibility
        ↓
expected building / site geometry
        ↓
PHYSICAL SITE ESTABLISHED
```

This is evidence-backed site identity and composition, not conventional geocoding.

### AI infrastructure identity

The same physical project can appear under different names across press releases, planning records, utility/PUC dockets, interconnection records, permits, tax agreements, LLC ownership records, local news, and company filings. Canon + Canon Geo should establish when those observations refer to the same physical site and preserve disagreement when they do not.

Target graph:

```text
BUILDOUT PROJECT
        ↓
developer / operator
        ↓
project aliases
        ↓
legal entities
        ↓
parcel assemblage
        ↓
exact geometry
        ↓
buildings
        ↓
permits / planning cases
        ↓
substation / utility
        ↓
interconnection / power capacity
        ↓
construction state
```

This can become a physical-identity substrate for AIBuildout rather than a separate one-off geocoder.

## 4. Claim object

Working concept only; naming/versioning must follow Canon schema doctrine before implementation:

```text
geo_entity_claim.v0

claim_id
as_of
name?
aliases?
address?
city?
county?
state?
postal_code?
market_or_msa?
lat_lon?
property_type?
units?
square_feet?
acreage?
year_built?
acquisition_date?
owner?
borrower?
developer?
operator?
legal_entity?
power_mw?
planned_buildings?
planning_case_id?
permit_id?
interconnection_id?
nearby_landmark_or_infrastructure[]?
other_typed_observations[]
source_evidence[]
```

Every observation retains provenance and evidence date. Missing fields are normal. Domain-specific observations should remain typed extensions rather than forcing all domains into one flat schema.

## 5. Candidate acquisition is separate from solving

Canon Geo should not become a web scraper or silently invent the world inventory.

Upstream acquisition/catalog/source operators provide a pinned regional inventory: parcels, assessor attributes, address points, building footprints, Overture, OSM, ownership records, permits, planning/zoning records, utility/interconnection records where available, and authorized private inventories.

Canon Geo consumes that evidence-dated inventory and solves within it.

Preserve three distinct questions:

1. **candidate reach** — did the bounded inventory contain the true asset/site?
2. **evidence admission** — which observations may constrain the solve?
3. **identity/composition solve** — what conclusion follows from admitted evidence?

An acquisition failure must not be reported as a solver failure.

## 6. Hard vs soft evidence

Prefer deterministic constraints wherever possible.

Deterministic/typed evidence can include exact geometry, parcel/building incidence, jurisdiction boundaries, unit counts where semantics align, acreage/square footage with declared tolerances, dates, parcel IDs, legal-owner IDs, normalized addresses, source-dated ownership relationships, planning/permit IDs, utility/interconnection IDs, and measured proximity to established infrastructure.

Candidate/probabilistic evidence can include property/project-name similarity, alias equivalence, assessor/zoning semantic compatibility, fuzzy owner/developer relationships, descriptive property-type equivalence, narrative proximity, and other soft attributes.

A model/Jev-style judgment may rank or type soft evidence, but model confidence alone must not mint a stable physical identity.

## 7. Potential Jev / model role

Useful bounded judgments include:

```text
"Arbors at Brentwood" ≈ "The Arbors of Brentwood" ?

assessor use code "MULTI RES > 20 UNITS"
compatible with "multifamily apartment community" ?

"Project Beacon" and a county planning case under an LLC name
plausibly refer to the same candidate project?

Does owner LLC A plausibly participate in the disclosed developer/REIT ownership chain?
```

Model output enters as candidate evidence/belief under explicit policy. Canon's solver/evidence semantics retain authority over physical-identity state.

## 8. NXRT blind evaluation

1. Freeze the NXRT portfolio at one evidence date.
2. Independently establish withheld street-address + parcel/building truth.
3. Do **not** provide addresses to Canon Geo.
4. Supply only selected public-company descriptors.
5. Acquire bounded regional inventories under pinned releases.
6. Run candidate reach and physical-asset resolution.
7. Compare to withheld truth.
8. Account for every declared property.

Measure candidate reach, forced-identity precision, supported-identity precision, abstention quality, parcel/building composition accuracy, source/evidence coverage, and accounting coverage.

Target **100% accounted**, not 100% guessed.

A result such as 31/36 forced correctly, 3 supported/not forced, 1 acquisition-incomplete, 1 unresolved, zero forced false positives, and 100% accounting would be strong even though resolution is below 100%.

## 9. Data-center blind evaluation

Choose a project whose public announcement does not hand us a clean address but whose physical site can later be independently validated.

1. Freeze evidence at a selected date.
2. Construct the claim only from public descriptors available then.
3. Withhold independently established parcel/address truth.
4. Acquire bounded county/municipal parcels, buildings and relevant evidence planes.
5. Resolve candidate parcel assemblages/buildings.
6. Return forced/supported/ambiguous/conflicted/unreachable state plus next evidence.
7. Compare to independently established physical truth after the run.

Measure parcel-assemblage recall, site-boundary accuracy, alias-consolidation precision, owner/developer link precision, false project merges/splits, evidence-date correctness, and accounting coverage.

A strong proof is:

> **We converted fragmented public observations about a project into an evidence-backed physical site without being given its address.**

## 10. Public Proof opportunities

### REIT

> **Starting only with public-company disclosures, we reconstructed the physical collateral underlying a public apartment REIT's mortgage portfolio.**

```text
REIT → mortgage → property/legal entity → apartment community → street address → parcels → buildings → exact geometry
```

### Data center

> **Starting from a project announcement with no usable street address, we established the parcels and buildings that make up the physical data-center site.**

```text
project mention → project/developer identity → legal entities/aliases → parcel assemblage → buildings → geometry → permits/power/utility evidence
```

Both are stronger than ordinary geocoding because physical identity is an evidence-backed outcome rather than an input assumption.

## 11. Other workloads

The same abstraction applies to CMBS collateral, bank CRE portfolios, insurance/mortgage holdings, permits/planning records, and other infrastructure projects.

## 12. Evidence Machine relationship

Canon Geo remains the physical identity/composition operator rather than absorbing Evidence Machine orchestration.

```text
public/private source evidence
        ↓
Evidence Machine inquiry
        ↓
Catalog / source capability discovery
        ↓
bounded regional acquisition
        ↓
Canon Geo
physical identity / collateral composition
        ↓
Evidence Machine
admissibility + coverage + conclusion accounting
        ↓
sealed outcome / collateral ledger
```

## 13. Why this may be a better primary use case

Address-first remains useful and is not removed. But address-first begins after one of the hardest identity questions has already been answered.

The incomplete-claim workload tests something closer to Canon's deeper purpose:

> **Resolve observations about real-world things into evidence-backed identity without silently guessing.**

It exercises regional candidate universes, candidate reach, multisource evidence, typed observations, constraint propagation, ambiguity/conflict, next-evidence logic, parcel/building composition, provenance, and 100% accounting.

Data centers strengthen the framing because a postal address may not even be the natural identity of the asset. A campus can span many parcels and buildings; physical composition itself is the outcome.

## 14. Near-term implementation posture

Work can begin without rewriting the controlling Canon Geo plan.

### Track A — one NXRT property

```text
withhold known address
→ collect only SEC-disclosed descriptors
→ acquire bounded city/market inventory
→ adapt existing evidence request
→ produce candidates
→ solve / abstain
→ compare with withheld truth
```

Then expand to five properties before attempting the full portfolio.

### Track B — one data-center project

```text
publicly announced project with no supplied address
→ freeze descriptors
→ bound county / municipality
→ acquire parcels + buildings + planning/ownership evidence
→ construct candidate parcel assemblages
→ admit typed evidence
→ solve / abstain
→ compare with independently established site truth
```

Do not begin with nationwide acquisition infrastructure. Choose geographies where evidence is accessible and prove resolution semantics first.

## 15. Success criterion

The first success is not "Canon Geo guessed the right address."

It is:

> **Given an incomplete, evidence-backed description and a bounded physical universe, Canon Geo either establishes the correct physical asset or explicitly accounts for why the evidence is insufficient to do so.**

That is the durable product abstraction.