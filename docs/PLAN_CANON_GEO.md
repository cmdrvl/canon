# PLAN_CANON_GEO — The Tile as a Compiled Constraint Object

> Status: **proposed full architecture with a partial E4 walking skeleton implemented**.
> **Course correction 2026-09-01:** §18 restates the product as collateral composition,
> evidence-dated existence, and named conflicts, and places every proposal in `IN`,
> `DEFERRED`, or `CUT`; §19 is the staged execution plan with frozen gates. Where §18
> defers or cuts something proposed below, §18 controls.
> The typed evidence compiler, exact parcel/building residual kernel, explicit
> parcel-default/building selection profiles, incidence factorization, bounded fallback,
> population evaluator, and profile-aware offline warehouse-row materializer exist.
> Deterministic H3 center-plus-halo work-unit materialization and
> cross-boundary ownership reconciliation now exist over explicit upstream home-cell
> assignments; controlled-halo candidate reach is positive in six NYC r8 strata and the
> stress-selected logical r9 child of each stratum. Parcel/footprint majority-incidence
> components are measured there first with NYC footprints and then with a lineage-aware
> Overture building plane. Citywide reach, source-to-source latent-building
> reconciliation, warehouse/client H3 parity, and final solver-component cost remain
> open. The source-generic agent control plane—typed question, regional inventory,
> resolution profile, deterministic plan, bounded offline run manifest, and residual-aware next
> evidence—is specified in
> [`CANON_GEO_AGENT_ARCHITECTURE.md`](./CANON_GEO_AGENT_ARCHITECTURE.md). Its typed
> question/capability/inventory/budget layer and protocol-neutral discovery/acquisition
> contracts are partially implemented. Deterministic offline `canon geo plan` now emits
> `canon_geo_plan.v0` as a Geo semantic overlay over one validated
> `canon.project.plan.v1` DAG; it plans only the current parcel/building composition
> profile, emits typed external requests for missing inputs, and never executes work or
> proves live source truth. Bounded offline `canon geo run` now consumes that plan,
> explicit local `NODE_ID:BINDING_ID=PATH` inputs, and optional `--satisfy`
> receipt checks, then delegates the five implemented Geo stages to the shared
> `canon.project.run.v2` runner and emits `canon_geo_run.v0`. The shared runner publishes
> immutable content-addressed manifest revisions with full-plan receipt prevalidation, but
> Geo run does not perform live acquisition, prove live source truth, mutate the immutable
> plan, clear acquisition blockers, or automatically replan during `geo run`,
> schedule concurrently across agents, recover crash-stale locks, or provide an
> inspection/next-evidence surface. The public CLI `--satisfy`
> check only validates an explicit acquisition receipt against explicit local input bytes.
> `canon geo replan-from-acquisition` can additionally materialize an immutable, plan-bound regional-inventory
> advancement for `live`, `COMPLETE`, full-region `canon_geo_warehouse_rows.v0` JSON artifacts
> with either the legacy unambiguous one-release/one-artifact binding inferred when
> receipt-native relations are absent, or the new multi-release/multi-artifact
> receipt-native relations mapping every local artifact to exactly one pinned release and
> every pinned release to exactly one artifact. Zero/partial/truncated execution,
> retained/fixture proof, untyped artifacts, and narrower subsets stay non-advancing. That
> artifact still does not mutate the base plan or clear its blockers in place: the command
> atomically writes the separate advancement sidecar and emits a new base-inventory-bound
> `canon_geo_plan.v0`. Geo inspect and
> residual-aware next-evidence control remain open. Geometry
> acquisition/ingest, temporal solving, knowledge compilation, and
> the complete E4/E5 populations do not exist. This does not change canon core: runtime
> lookup remains exact registry lookup.
>
> Date: 2026-08-15. Derived from an adversarial multi-model design session (see
> [Provenance](#15-provenance-and-what-is-not-yet-verified) — **the ~50 academic citations
> below have NOT been independently verified**).

---

## Review state and precedence

> Last status reconciliation: **2026-08-31**. This is a review-navigation layer, not a
> claim that the full architecture ships. The implemented walking skeleton is named
> explicitly above and in the controlling-state table; everything else remains proposed.

This document deliberately preserves hypotheses that later measurements falsified. That
history is evidence, but it creates a precedence problem for a reader who encounters the
original claim before its correction. Read the document under these rules:

1. An inline **CURRENT STATUS** note controls the prose immediately below it.
2. A later scoped measurement supersedes an earlier estimate only for the population and
   predicate it actually measured. It does not silently generalize beyond that scope.
3. `PROPOSED` means architecture to review; `MEASURED` means a recorded result with a
   declared denominator; `VERIFIED` means checked against the named primary source or
   executable contract; `FALSIFIED` means the stated claim must not be used; and `OPEN`
   means no implementation or product claim may depend on the answer yet.
4. Appendix status labels describe evidence maturity, not shipped Canon capability.

The controlling state entering the main review is:

| Topic | Current controlling state | Authority |
|---|---|---|
| Product boundary | Core Canon remains exact registry replay; GEO is a build-time workbench. | `AGENTS.md`, `README.md`; binding boundary |
| Agent operating model | The target surface is question + regional inventory + resolution profile + deterministic budget -> plan -> immutable run revisions -> typed answer/explanation/next action. Leaf Geo commands remain independently callable, `canon geo capabilities --emit json` is shipped as an offline/read-only capability artifact, `canon geo plan` now ships as an offline/read-only planner that emits `canon_geo_plan.v0` over one shared `canon.project.plan.v1` DAG, and `canon geo run` now ships as a bounded offline executor for that current five-stage parcel/building composition plan. The run path emits `canon_geo_run.v0`, accepts explicit local input bindings, and treats optional CLI `--satisfy` arguments only as receipt-vs-local-bytes validation guards. The library can emit a separate plan-bound inventory advancement for live, complete, full-region `canon_geo_warehouse_rows.v0` JSON artifacts using either the legacy inferred one-release/one-artifact binding or receipt-native multi-release/multi-artifact relations that cover every pinned release and every local artifact one-to-one; the agent must explicitly feed that immutable snapshot through the base-inventory-bound replan path to produce a new plan. Valid but untyped CSV/JSONL receipt artifacts remain satisfactions, not inventory advancements. It preserves unsupported grains as typed blockers/actions and never mutates the old plan or clears its blockers. Native sources also declare whether they can contribute a stable alias or evidence only; evidence-only sources can support their declared evidence class but cannot satisfy a stable-identity claim. It does not acquire live data, attest live source truth, expose the inspect surface, issue ready-node claims, recover crash-stale locks, or schedule concurrently across agents. Source instances belong in adapters/inventories, never core branches. | `CANON_GEO_AGENT_ARCHITECTURE.md`; typed control/discovery/plan/run contracts `PARTIAL`, inspection/concurrency/next-evidence control `OPEN` |
| Shared project substrate | Library-level `canon.project.plan.v1` and `canon.project.run.v2` provide manifest/lock DAGs, receipts, resume, invalidation, lifecycle, and workspace policy. The project CLI exposes lock refresh and pure planning; public project run validates/reuses v2 receipts and executes pending nodes only through registered internal offline executors. The first narrow `copy-file-v1` executor proves positive dispatch with digest-checked inputs and one declared output. Geo plan reuses the project DAG as a typed semantic overlay, and bounded `geo run` delegates the five current Geo stages to that shared runner with typed local input bindings. Node receipts are retained in content-addressed storage, and the shared runner publishes immutable content-addressed manifest revisions with previous-hash lineage after prevalidating receipts across the full plan before selected execution; cooperating writers use per-slot publication locks before selecting one canonical receipt/output, and semantically equivalent receipts deduplicate only when their operational bindings agree. This is convergence substrate, not the parallel work protocol: output-plus-receipt and multi-output transactionality, crash-stale lock recovery, ready-node claims, Geo inspect, live acquisition, and concurrent scheduling remain open. Geo must extend this substrate rather than create a second scheduler. | `src/project/{manifest,lock,plan,run,receipt,lifecycle,workspace}.rs`, `src/geo/run.rs`; reusable positive executor seam, bounded Geo execution, and runner manifest revisions `PARTIAL`, claims/inspect/concurrency `OPEN` |
| N-source row composition | `canon geo link-sources` now materializes three or more named local CSV sources through the existing entity multisource kernel. Geo requires exactly one target, at least one bounded reference, permits peers, refuses a globally canonical vendor role, defaults to the complete comparison graph, enforces per-pair budgets, emits anchor-conflict abstentions, and content-hashes every input and the merged rows. The semantic artifact hash excludes publication paths and is compatible with `EntityArtifactReference`; source count remains provenance rather than evidence weight. This is row composition, not spatial candidate reach, constraint admission, or solving. | `src/geo/multisource.rs`, `src/entity/multisource.rs`, `canon_geo_multisource_request.v0`, `canon_entity_multisource_link.v1`; implemented build-time workbench contract |
| Offline row bridge | `canon geo materialize-evidence` deterministically groups release-pinned profile-permitted parcel/building rows, parcel incidence where declared, rho-contract, and immutable source-record rows into `canon_geo_evidence_request.v0`; duplicate grains and conflicting observation rows refuse, and the production evidence compiler validates the result. The backward-compatible default parcel profile still requires parcel candidates; explicit building profile permits an empty parcel universe, rejects parcel rows/incidences, and preserves building-only evidence. H.7 also has a profile adapter from PIP-block warehouse rows directly to a content-bound `observed_snapshot`; the current 142-row / 71-subject MCP result materializes, but its 70 solver inputs contain no admitted observations and therefore yield 69 ambiguous abstentions plus one unreachable structural singleton. These paths perform no acquisition, source-record multiplicity remains provenance rather than constraint weight, and an observed snapshot lacking executor query identity cannot become `LiveComplete`. | `src/geo/materialize.rs`, `canon_geo_warehouse_rows.v0`, `canon_geo_h7_pip_block_population_batch.v0`; implemented build-time bridges, H.7 evidence admission still open |
| Measurement receipt integrity | The companion `canon_geo_measurements` binary emits a deterministic offline plan or checks supplied result artifacts and receipts against the pinned B/C/D/F manifest. It recomputes local source-SQL bytes, normalized executed-query-text bytes, artifact bytes, an unordered canonical result-set digest, row-derived denominators, and declared sanity fields. A successful row is only `receipt_consistent`: the operator-supplied proof class is reported separately, and the runner does not attest live execution, query-history provenance, or source authenticity. | `src/bin/canon_geo_measurements.rs`, `scripts/geo_measurements/manifest.json`; offline integrity contract `IMPLEMENTED`, live provenance `OPEN` |
| Tile work and boundary ownership | `canon geo materialize-home-cells` derives release-bound h3o cells from fixed-decimal WGS84 representative points, retains geometry/transform bindings, nine-point coordinate-envelope probes, the minimum probe-covering halo, and claimed-cell parity. The current v1 tile contracts also preserve each source instance's pinned release, native entity level, identity participation, and plan-shaped inventory reference through home-cell assignment, center-plus-halo work, and reconciliation; one source instance cannot silently mix release/scope/inventory/method/transform bindings, observation-only features cannot become candidate members, and a feature cannot be promoted across entity levels. Decision semantics are explicit: `composition` may contain mixed native levels and EvidenceOnly-only membership but carries no alias-mint authority and remains available without inventory-lineage validation. `stable_identity` names one entity level, requires every member at that native/candidate level and at least one StableAlias participant there, recomputes the supplied canonical regional inventory's semantic and planning hashes, and requires every member's source instance, release, native scope, and inventory reference to agree exactly. Stable decisions and their artifact retain those hashes as an inventory-relative authority boundary, not external or world truth. The geometry digest is validated as a binding but cannot be recomputed because this artifact intentionally omits geometry bytes. `canon geo tile-work` materializes one budgeted H3 center-plus-halo work unit. Each reconciliation proposal repeats and is checked against the domain-separated digest of its embedded canonical work unit, and `canon geo reconcile-tiles` emits one owned decision per declared semantics plus canonical member set. That check proves deterministic association to caller-supplied bytes; without an executor-issued receipt it does not prove an external solver consumed the work unit. Reconciliation refuses missing owners, halo-only decisions, unavailable or relabeled members, inventory-authority laundering, cross-level stable-identity laundering, and differing payload digests for the same semantic scope. H3 supplies blocking and ownership only, never geometric truth. Historical v0 schemas remain published but are not accepted by the current v1 command surface. Fresh v3 rows expose complete centroids but null source-plane H3 fields, correctly requiring this derived sibling. D.11 finds positive k1 reach in six r8 strata and one deliberately dense r9 child per stratum, with explicit Canon neighbor disks. It does not establish citywide recall, global h3o parity, client-layer coverage, or solver-payload interpretation. | `src/geo/tile.rs`, `canon_geo_home_cell_*.v1`, `canon_geo_tile_work_*.v1`, `canon_geo_tile_reconciliation*.v1`; executable assignment/ownership contract `IMPLEMENTED`, stratified bounded reach `MEASURED`, generalization `OPEN` |
| Decision object | Entity-grain backbone and residual count with explicit scope and exactness; typed fallback when either is incomplete. Ledger keys are alias projections. | §§9, 10.2, 16.1; Appendix L.5 |
| Candidate problem | Point re-ranking is not the dominant measured failure. The unresolved solver question is collateral composition over parcel/building sets. | Appendices L–M; `MEASURED`, with E4 `OPEN` |
| Footprint→parcel predicate | Strictly more than 50% of computed footprint geometry inside computed parcel geometry, within an explicitly interior-disjoint parcel stratum; asserted area fields are observations, never denominators. Candidate reach is independent: a footprint and its majority parcel may have different H3 home cells. The fresh NYC+Overture rerun finds k1 equal to the complete parcel reference in all twelve measured strata for both footprint planes. Overlapping legal parcel hierarchies still require typed crosswalks. | Appendices D.9–D.11 and F.6; corrected predicate/reach split and stratified two-footprint-source halo `MEASURED`, FEMA/client rerun `OPEN` |
| Decomposition | Legacy mixed-denominator runs produced forests and parcel stars up to 71 variables. D.11's fresh geom-v3 NYC-footprint graph remains a forest in all twelve measured r8/r9 strata, with maxima 3–65 at r9 and 4–71 at r8. F.6 adds Overture observations and remains a forest, but raw observation maxima rise to 5–118 at r9 and 7–128 at r8. Those are parcel/center-observation predicate-incidence components, not deduplicated latent buildings or final solver widths: source reconciliation and additional evidence may merge or couple them. Canonical overlap-aware solver decomposition remains open; solver incidence factorization is implemented independently. | Appendices D.11 and F.6; stratified multi-source predicate incidence `MEASURED`, multi-source solver incidence `OPEN` |
| Work-unit cost | The 200-feature, 0.5 s/tile, and 140 CPU-hour national figures are not supported. D.11 measures two-source r9+k1 work units of 378–4,670 nodes. F.6's raw three-plane work units are 596–7,015 nodes at r9, while predicate-incidence maxima are 5–118. This supports component-wise solving but also proves that raw source rows must not be mistaken for latent-building variables; compilation, source reconciliation, FEMA, and client-layer costs remain unbenchmarked. | Appendices B, C, D.11, F.6, G; original figures `FALSIFIED`, replacement runtime model `OPEN` |
| Address evidence | PAD materially repairs address representation and restores street-absence refutation, but is evidence rather than an oracle. `canon geo materialize-address-evidence` now preserves the parse forest and PAD-membership audit, unions supported readings into one parcel existential observation, abstains on chimeras/no support, and binds each source-record association to Canon's hash of the normalized PAD-member payload. The hash prevents id-only payload substitution but does not authenticate upstream bytes or make the artifact live truth. Time-scoped observations remain diagnostic in composition v0. | `src/geo/address.rs`, `canon_geo_address_parcel_*.v0`; offline bridge `IMPLEMENTED`, full residual replay/tile compatibility `OPEN`, Appendix M `MEASURED` on NYC PAD 26B |
| Evaluation ladder | E1–E3 are complete. E4 has an exact factorized residual solver over admitted evidence (bd-2kjx.1–.3); the E4 population numbers and the E5 non-NYC evidence-tier curve remain the decisive gates. | §17 and Appendix L; E4/E5 `OPEN` |
| E5 geography preflight | Franklin County, Ohio (`39049`) now has a real parcel-backed successor to the immutable 2026-08-31 thin-tier preflight. Pinned current inputs are bridge build `ce3953ac-c2d4-4b48-bf02-29f0cf341389` and Franklin parcel release `hub-de09f99cce0bcae7142d6d2e26582fd3-25` / `2026-09-01`. Of 494,704 landed parcels, 494,043 pass the declared source/derived geometry admission. H3 feature coverage gives every one of 151 property subjects a nonempty block; Snowflake GEOGRAPHY PIP reaches 147, with 146 unique and one two-parcel case. The four misses are 3.006–22.221 m from the nearest blocked parcel and none is rescued by invalid-retained geometry. A seeded live row also traversed original EPSG:3735 WKB → independent digest verification → Canon fixed-point materialization: 29 decoded / 28 canonical vertices, ≤1 µm decimal admission loss and ≤499 µm lattice snapping. These are candidate-reach and source-byte transport results, not precision, exact-local parity, solver correctness, or an evidence-tier operating point. Successful MCP envelopes still omit query ids, so durable live receipt promotion remains open. The applicable FEMA Ohio partition remains `2023-05-02`; vintages are pinned per geography. | `e5_franklin_county_parcel_candidate_reach.sql`, `e5_franklin_county_live_geometry_probe.sql`; parcel candidate reach and one seeded source-byte path `MEASURED`, generic core isolation `TESTED`, E5 `OPEN` |
| Time semantics | Evidence admissions preserve whole-day valid-time intervals, and v0 deliberately keeps every time-scoped observation diagnostic because composition has no query-as-of domain. Allen/STP inference is not implemented. | §§3, 7, 16.3; compiler contract implemented, temporal solver `OPEN` |
| Current precision claim | The 96–98% entity-grain answered-point estimate is provisional and truth-instrument-limited; Appendix M indicates residual contamination. | Appendices L.6 and M.5; `MEASURED`, not a release claim |
| Product thesis | Collateral composition at parcel and building grain, evidence-dated physical existence, and named source conflicts (§18.2). Point re-ranking is `CUT`. Honest abstention is required but not differentiating. | §18; binding scope |
| Solver scope | Extensional exact kernel retained as backend. Propagators (additive band, cardinality, exclusivity) and explanation artifacts (minimal core, correction sets, counterfactual separation) are `IN`; compiled representations, latent-slot symmetry breaking, Allen/STP, and VeriPB are `DEFERRED` with named triggers (§18.3). | §18.3, §18.5; `IN` items owned by beads, `DEFERRED` items hold P4 placeholders |
| Imagery and map evidence | Licensed orthos, 3DEP, NAIP, NOAA ERI as pinned observer inputs; observers emit typed observations with characterized error through `rho`; first uses are truth adjudication and the evidence card, solver input third. Commercial basemaps and location-proposing models are `CUT`. | §18.4, Appendix J; `PROPOSED`, beads created 2026-09-01 |

The 2026-08-29 and 2026-08-30 live home-cell receipts are preserved in
`scripts/geo_measurements/README.md`. It includes complete v3 null-H3 controls,
a 10/10 bounded footprint h3o parity sample, a deterministic five-row MapPLUTO
v3 artifact, the two-cell controlled-halo result, the twelve-stratum r8/r9
NYC-footprint and Overture reach/predicate-incidence measurements, and the correction that
`882a100d8bfffff` is dense Brooklyn rather than Manhattan. None is promoted to
global candidate-recall proof or final solver cost.

---

## Agent-operable system boundary

[`CANON_GEO_AGENT_ARCHITECTURE.md`](./CANON_GEO_AGENT_ARCHITECTURE.md) is the normative
operating model for the whole workbench. This plan remains authoritative for its
mathematics, source-admission discipline, empirical measurements, and E1–E5 gates. The
agent architecture is authoritative for how those capabilities compose into a controllable
system.

The abstraction tower is:

```text
GeoQuestion
  + compiled Canon capabilities
  + regional evidence inventory
  + source-generic resolution profile
  + deterministic resource budget
    -> costed execution DAG
    -> pinned local source adapters
    -> bounded tile + controlled halo sections
    -> independent candidate-universe/reach state
    -> rho admission + constraint IR
    -> actual incidence components
    -> small exact residuals or typed deterministic fallback
    -> cross-boundary reconciliation
    -> separately reported coverage/reach/solver/truth/cost planes
    -> explanation and cheapest potentially decision-changing next evidence
    -> review-gated registry proposal -> exact replay
```

The target durable agent memory is a content-addressed `canon_geo_run.v0` manifest
indexing immutable artifacts, phase states, budgets, blockers, reusable work, and exact
next commands. The current shipped run artifact is narrower: it records the bounded
five-stage run, typed local input refs, output refs, grain states, blockers, next actions,
deterministic usage, optional `canon.project.run.v2` report, and operational observations.
It reuses verified project receipts inside one work directory when effective input hashes
match, and the shared runner publishes immutable content-addressed manifest revisions
after prevalidating receipts across the full plan. Its Rust API also offers an opt-in
deterministic JSONL progress writer that reports
validated reuse before pending execution, monotone phases, committed artifacts, counters,
wait/cancellation/failure state, and leaves the semantic run bytes unchanged. That stream
is not yet a public CLI/schema capability. Ready-node claims, crash-stale lock recovery,
cross-agent scheduling, live acquisition, and inspection are still open. Paths, clocks,
worker order, and machine identity do not enter semantic hashes.

This operating model adds no new epistemic shortcut. More admitted hard evidence narrows
the model set or makes it empty. Source count is provenance, not independent information.
Model-count reduction is counterfactual separation, not expected value of information
without calibrated outcome probabilities. Exactness remains relative to the admitted,
quantized representation. Candidate reach, constraint soundness, solver correctness,
reconciliation confluence, and truth quality remain different gates.

The shipped `canon geo stack-evidence` seam makes that accretion operational without
making a provider or parcel layer mandatory. Its base is any bounded labeled population;
its overlay names cases and carries only versioned rho contracts and observations. It
cannot change truth, candidate universe, composition profile, or solver budgets. Every
artifact content-binds and retains the canonical base, overlay, and result, and is
replay-validated before it can be stacked again or evaluated. Exact reuse is idempotent;
contract/observation redefinition and semantic duplicate observations under alternate IDs
refuse. Hard, soft, and diagnostic admissions are counted separately, while source-record
volume is explicitly provenance rather than confidence. This is the generic evidence
tower between regional materializers and exact bounded solving: address, footprint, deed,
area, imagery, or future sources may all target it by emitting the same typed overlay.

The target control surface is deliberately small:

```text
canon geo capabilities --emit json
canon geo plan --question ... --capabilities ... --inventory ... --profile ... --budget ...
canon geo run --plan ... --work-dir ... [--input NODE_ID:BINDING_ID=PATH]... [--satisfy REQUEST_ID=RECEIPT.json]...
canon geo replan-from-acquisition --base-plan ... --base-inventory ... --question ... --capabilities ... --profile ... --budget ... --satisfy REQUEST_ID=RECEIPT.json --local-artifact LOCAL_ARTIFACT_ID=PATH... [--result DIGEST_ID=PATH...] --advancement-out ...
canon geo inspect --run ... [--compare ...] [--recommend-next]
```

The first command is shipped as an offline/read-only leaf that emits
`canon_geo_capabilities.v0`; `geo plan` is shipped as an offline/read-only compiler for
`canon_geo_plan.v0` over one validated `canon.project.plan.v1` DAG; and `geo run` is
shipped as a bounded offline executor for that current five-stage plan. It delegates
`materialize-home-cells`, `tile-work`, `materialize-evidence`, `compile-evidence`, and
`solve` to the shared `canon.project.run.v2` runner, hashes explicit local
`--input NODE_ID:BINDING_ID=PATH` bytes into the effective project DAG, and emits
`canon_geo_run.v0`. Optional CLI `--satisfy REQUEST_ID=RECEIPT.json` arguments are
validation guards only: they check an explicit acquisition receipt against explicit local
input bytes. They do not mutate the immutable plan, clear acquisition blockers, update the
inventory, or replan. The public `geo replan-from-acquisition` command materializes a
separate inventory-advancement artifact only when the plan inventory hashes match exactly,
the receipt is `live` `COMPLETE`, the acquisition subset is full-region, and every local
artifact is usable `application/json` under `canon_geo_warehouse_rows.v0`. The legacy unambiguous case can
infer the binding from one pinned release and one local artifact when receipt-native
relations are absent; multi-release or multi-artifact advancement requires receipt-native
artifact-release relations that cover every pinned release and every local artifact
without duplicate or cross-product ambiguity. A valid untyped CSV/JSONL receipt can
satisfy its acquisition request but cannot make a source planning-ready. Retained/fixture
proof, zero/partial/truncated execution, and narrower subsets remain non-advancing.
The command writes the advancement as an explicit sidecar and emits a new
base-inventory-bound plan whose inventory snapshot and inputs reflect that acquired
evidence. `geo run`
currently operates only within the
parcel/building composition-profile limit: omitted/default `parcel` preserves the
non-empty parcel universe requirement, explicit `building` permits a parcel-free building
universe, and unsupported grains remain separately typed. `geo inspect` remains an
**OPEN design target**, not a current CLI claim. Existing leaf commands remain
independently callable and machine-described. The public library implements and validates
`canon_geo_discovery_request.v0`, `canon_geo_acquisition_request.v0`, and
`canon_geo_acquisition_receipt.v0`; the planner may emit those typed requests for missing
local inputs, and the run path may validate supplied receipts against supplied local bytes,
but Canon still performs no live acquisition or live proof. Composite execution stays
offline; missing network inputs
become typed discovery/acquisition requests when their release/as-of selectors are
sufficient, and otherwise remain explicit gaps rather than hidden provider calls. External
executors may use Reveal catalog discovery,
Snowflake, S3, or future services, but Canon validates one protocol-neutral receipt carrying
release, bounded subset, conditional geometry projection, pagination state, query/request id,
denominators, digests, and proof class.

---

## 1. The thesis

A tile is a **Waltz scene**.

Roughly 200 noisy local observations of one physical block, from 4–6 sources with no shared
identifier, governed by physical laws that admit only a few globally consistent
interpretations. That is precisely the problem classical constraint reasoning was invented
for:

- Waltz (1972/75) — resolving ambiguous scene labellings from local constraints
- Montanari (1974) — *"Networks of constraints: fundamental properties and applications to
  picture processing"*
- Rosenfeld, Hummel & Zucker (1976) — *"Scene labeling by relaxation operations"*

**The industry attacks this problem with a spatial join and a trigram index.**

We do not ship a point estimate with a score. We ship a compiled object `T` whose
properties are *provable* rather than asserted.

| Property | Mechanism | Authority |
|---|---|---|
| Fixpoint unique regardless of application order | monotone, contracting, correct propagators on a finite lattice | Tarski 1955; Cousot & Cousot 1977; Apt 1999 |
| Compiled semantics can have a canonical normal form; byte identity additionally requires a frozen serializer | reduced OBDD under a fixed variable order, or compressed/normalized SDD under a fixed vtree. General d-DNNF is not canonical. | Bryant 1986; Darwiche 2011 (SDD) |
| Adding admitted hard evidence can only narrow the model set; it may expose a contradiction by making that set empty | `Models(T ∧ c) ⊆ Models(T)` plus a separate non-emptiness check; entailment alone must not label an empty successor a healthy refinement | Darwiche & Marquis 2002 |
| Abstention is the residual, not a threshold | the answer is the model set; a singleton is a decision, a doubleton is an honest doubleton | — |
| Empty model set is a **proof of source defect** with a minimal ordered blame set | MUS via preference-ordered QuickXplain; repairs via hitting-set duality | Reiter 1987; Junker 2004; Liffiton & Sakallah 2008 |
| Every conclusion explainable by naming evidence | minimal environment supporting the conclusion, computed on demand | de Kleer 1986 (ATMS) |
| The whole run is independently machine-checkable | pseudo-Boolean proof log covering global-constraint propagation and symmetry breaking | Gocht, McCreesh & Nordström 2022 (VeriPB) |

---

## 2. Why the previous two architectures failed

Both prior designs picked a channel to **propose** candidates and demoted the other to
**confirm**. Both were red-teamed and destroyed.

**Address-proposes fails.** The parcel layer stores one representative `ADDRESS` per lot,
while large and corner lots legitimately carry many. Measured: geocode `1633 BROADWAY` vs
lot `1657 BROADWAY`; `9 WEST FORDHAM ROAD` vs lot `2167 GRAND CONCOURSE`. The true answer
is frequently **unreachable from the string**, so grounding fails silently and the system
biases toward whichever reading happens to match the stored representative — producing
wrong answers rather than abstentions.

**Geometry-proposes fails.** Interpolated geocodes sit in the roadbed contained by nothing;
one measured case parsed to the wrong street 1.8 km away at ROOFTOP confidence.

**The dilemma was an artifact.** It arose from using noisy evidence in its *unsound
direction* and then needing a second channel to clean up. Fix the direction and the
dilemma dissolves. There is no proposer.

### 2.1 The checksum idea, and why it was demoted

An earlier proposal treated asserted attributes (size, year, count) as **parity bits** on a
parse — pick the reading whose implied physical footprint reconciles. The red team killed
it as a *decider* with an information-theoretic argument:

```
acceptance half-width w ≈ 0.12   (minimum honest; covers the measured NRA/gross gap)
plausible size range 5,000–2,000,000 sf = 400×
distinguishable bins = ln(400)/ln(1.27) ≈ 25
usable information   = log2(25) ≈ 4.6 bits      (3.5 bits at realistic w = 0.25)
```

Other attributes are near-zero **conditional on the tile** — competing readings are lots on
the same block, homogeneous in age and class. Total: **6–9 bits, generously.** Isolating
one reading from 10⁴ needs 13.3.

Worse, the discriminating power is **anti-correlated with case difficulty**: where readings
differ wildly, grounding already killed the bad ones; where they differ subtly, the sums
differ by less than measurement noise. And an error-correcting code asked to correct beyond
its distance does not degrade gracefully — **it confidently miscorrects to the wrong
codeword.**

**Verdict: size is retained as one constraint among many, contributing its ~4 bits. It is
not a decider.** This was an over-promotion, not a wrong idea.

---

## 3. ρ — the soundness discipline

**This is the single most important design rule and it is what makes a hard-constraint
frame survive noisy sources.**

> Every source attribute is admitted to the solver only through a declared, versioned
> relaxation operator **ρ** that maps the raw value to the *weakest constraint the source
> can actually support*.

| Raw evidence | Naive (unsound) reading | ρ-image (sound) |
|---|---|---|
| Geocode `g`, `interpolated` | "the property is at `g`" | "footprint intersects the disc of radius `r=150 m` about `g`" — nearly vacuous, which is **correct** |
| Geocode `g`, `rooftop` | same | `r = 8 m` — sharp, and legitimately so |
| Parcel `ADDRESS = "355 E 12 ST"` | "this lot's address is 355 E 12 St" | "355 E 12 St is *one of* this lot's addresses" — membership, never functional equality |
| Query address `199 First Ave` | "match a lot whose ADDRESS = 199 First Ave" | "*some* member of the collateral set fronts First Avenue at 199" — existential over the set variable |
| `BLDGAREA = 214,300` | "GLA is 214,300" | source-asserted gross above-grade area in the source's declared unit; never mix with exact geometry-derived area. Any net-rentable relation stays diagnostic until a population, calibration artifact, and falsification rule make its band admissible. |
| `OWNERNAME` equal after normalization | "same owner" | "these lots *may* form an assemblage" — permits, never forbids |
| `OWNERNAME` different | "different owner ⟹ not assembled" | **no constraint at all** |
| FEMA county coverage 92% | unused | `gcc` lower bound: ≥ `⌈0.80·K⌉` slots carry a FEMA observation |

### 3.1 Two consequences

**Every noisy channel is admitted only in its sound direction.** Address evidence never
excludes, it only requires existence. Ownership never separates, it only permits. Geocodes
never locate, they only bound.

**Theorem (trivial, and the whole business).** If every ρ is sound — the true world
satisfies `ρ(v)` whenever the source reports `v` — then the true assignment is in the model
set. Therefore:

> **An empty model set is a proof that at least one source violated its own published error
> model.**

Not "the sources disagree." Not "the join failed." A *proof*, attributable to a minimal set
of source records, that a specific vendor's declared tolerance was breached on a specific
parcel. **That is a falsifiable claim you can put in an email to Overture, FEMA, or a
servicer.**

The implemented v0 admission contract makes the premise inspectable rather than accepting
a caller-supplied `sound=true`: logical relaxations name their invariant; empirical bands
name a population, calibration digest, and falsification rule. This is provenance and a
falsifiable claim, **not a proof that the named invariant is actually sound**. Population
evaluation must still record every representable truth excluded by admitted hard evidence.
Time-scoped observations are preserved but remain diagnostic until the composition query
has an explicit as-of domain; otherwise an interval fact would be silently projected into
timeless identity. Compilation admissions retain the typed observation itself, so `solve`
can recompile and verify one-to-one source-observation-constraint parity before attaching a
content digest. Every rho contract also carries sorted upstream lineage identifiers so
shared ancestry is visible; different lineage labels are not proof of statistical
independence. The digest proves artifact identity and integrity, not source truth.

### 3.2 The band-versus-threshold rule

> **A threshold selects. A band restricts.** A wrong threshold silently produces a wrong
> answer. A wrong band produces an empty model set — a detected, attributable, reportable
> failure. **The system audits its own error models.**

Named price: **wider bands mean larger residuals.** We resolve fewer tiles to a singleton
than a competitor willing to guess. Paid in abstention, which is a first-class output.

---

## 4. Integer geometry

Every coordinate is projected into a **per-tile fixed local integer frame** and snapped to
millimetres in `i64`. Projection constants per H3 cell are precomputed once and shipped as
versioned data, so at decision time projection is a table lookup plus an exact integer
affine map.

**No transcendental function, no floating-point value, and no `f64` comparison appears
anywhere in the decision path.**

> **CURRENT STATUS — PROPOSED, AND “EXACT” HAS A BOUNDARY.** Integer arithmetic can make
> decisions exact with respect to the serialized, quantized local coordinates. It does not
> make the source survey infallible or the affine approximation geodetically exact.
> Projection and snapping need a measured error envelope. Polygon clipping can introduce
> rational intersection vertices even when every input vertex is integral, so area-majority
> needs an exact rational/scaled construction or a declared conservative boundary rule;
> integer orientation predicates alone do not settle it.

> **IMPLEMENTATION STATUS — EXACT TOPOLOGICAL KERNEL LANDED 2026-08-28 (`bd-15ba`).**
> `src/geo/geometry.rs` implements checked `i128` orientation, closed-segment
> intersection, simple-ring validation, exact integer twice-area, and point-in-ring with
> an explicit `interior` / `boundary` / `exterior` result. The dependency decision for
> this subset is **no external geometry crate**: the small integer kernel is the more
> auditable exact implementation once coordinates have crossed the tile artifact boundary.
> Neutral geometry machinery lives in the Geo workbench core; provider conventions, CRS
> selection, projection parameters, and domain-specific predicate policy remain in
> versioned tile/profile inputs. The Linux/macOS CI matrix runs a deterministic
> boundary-adjacent suite including one-millimetre offsets, translation, and ring reversal.
>
> The predicate kernel alone does **not** complete the geometry value/materialization
> contract (`bd-16r1`) or area-majority clipping. Exactness is relative to accepted
> quantized coordinates, never a claim of exact world geometry.

> **IMPLEMENTATION STATUS — EXACT SIMPLE-RING AREA MAJORITY LANDED 2026-08-30
> (`bd-2b9d`, IN PROGRESS).** `src/geo/geometry.rs` now computes the strict
> geometric-over-geometric `intersection_area > footprint_area / 2` predicate for two
> validated simple rings in the same local integer frame. It ear-triangulates each ring,
> constructs triangle-intersection vertices as checked exact rationals, and sums computed
> geometric area without source-asserted area fields, floating point, or epsilons. Exact
> half is false. Mixed frames, arithmetic overflow, and unsupported topology refuse with
> typed errors rather than falling back to an approximate answer. Adversarial tests cover
> rational cuts on both sides of half, partial collinear overlap, concave footprints and
> parcels, reversal/translation invariance, mixed frames, and overflow.
>
> This is deliberately a **bounded simple-ring kernel**, not yet the whole production
> parcel/footprint predicate. Polygon holes and multipolygon aggregation still need an
> explicit decision-domain composition rule; rational-denominator growth and runtime must
> be measured on real source-plane geometry; and candidate-complete tile + controlled-halo
> replay must precede any recall or decomposition claim. A typed overflow or unsupported-
> topology result is an abstention, not evidence that a footprint lacks a majority parcel.

> **IMPLEMENTATION STATUS — TYPED ARTIFACT BOUNDARY LANDED 2026-08-28 (`bd-16r1`, IN
> PROGRESS).** `src/geo/geometry_value.rs` admits source coordinates as fixed-scale decimal
> strings rather than binary floats, applies a versioned checked-integer affine frame, and
> snaps exact rational results to millimetres with ties-to-even. The artifact carries source
> CRS, local-frame id, coordinate unit/scale, vertex count, bbox, projection provenance, the
> exact maximum snap-error fraction, and a separate declared projection-error envelope.
> Point, polygon, and multipolygon values have canonical bytes: exteriors are CCW, holes are
> CW, the lexicographically smallest vertex starts each ring, explicit closing vertices are
> omitted, and holes/polygons are sorted. Documented adjacent duplicates normalize away;
> unclosed, degenerate, non-simple, intersecting, or topology-changing results refuse.
> Non-finite/excess-precision coordinates, mixed CRS, antimeridian crossings, invalid frame
> digests, arithmetic overflow, raw vertex excess, and canonical geometry-byte excess also
> have typed refusals. Decision geometry is never simplified or truncated to meet a budget.
>
> The deterministic parcel-scale test measures a 499 µm maximum snap on a 5 m geometry;
> with a separately declared 200 µm projection envelope the serialized audit reports a
> conservative 420 ppm endpoint-distance error bound. That proves the loss accounting and
> canonical byte path, **not** that a real H3 frame achieves the declared projection error.
>
> **IMPLEMENTATION STATUS — RELEASE-PINNED SOURCE-PLANE BRIDGE LANDED 2026-08-29
> (`bd-16r1`).** `canon geo materialize-warehouse-geometry --rows` consumes exported
> `NYC_DCP_MAPPLUTO_GEOM_V3_EXT`-shape rows offline. It recomputes the SHA-256 of canonical
> base64 ISO WKB before decoding, admits only 2D point/polygon/multipolygon geometry, and
> rejects a mixture of releases, archive digests, geometry-contract versions, CRS/SRID, or
> transform executions. IEEE-754 WKB coordinates cross an explicit, measured first
> quantization boundary into fixed 9-decimal source units; the exact US-survey-foot ratio
> `1,200,000 / 3,937` then maps EPSG:2263 source coordinates into local integer millimetres.
> The tile request carries an explicit versioned source origin. It is deliberately not
> derived from the current row bounds, because that would move every prior local coordinate
> when a later evidence row expands the bounds. The frame-parameter digest depends only on
> the frame definition, not on row membership or release metadata.
>
> A fresh 26v2 MapPLUTO v3 source row was decoded through the CLI: its declared WKB SHA-256
> matched recomputation, 20 raw vertices normalized to 19 canonical vertices, WKB-to-
> 9-decimal loss rounded up to 1 µm, and fixed-decimal-to-millimetre snapping rounded up to
> 491 µm. Repeated fresh-process output was byte-identical (SHA-256
> `b090c157aa37cd72c67d726f2f5bf9f829e9ff9e00b297769368626bb444ec59`). The source-plane affine declares
> zero projection error because it is only exact translation and unit conversion; the pinned
> source-to-WGS84 execution/definition ids are retained as sibling-plane provenance and its
> measured transform disagreement is never summed into source-plane local geometry.
>
> This proves the bounded source-WKB-to-local-integer path for an observed v3 row and keeps
> the two loss planes separate. It does **not** prove source survey accuracy, world truth,
> all-row validity, candidate recall, H3 assignment parity, or exact area-majority clipping.
> Those population measurements and predicates remain separate downstream gates.

Arithmetic: a 1 km tile spans ~10⁶ mm, coordinates ~2×10⁶. Shoelace terms ~4×10¹²; summed
over a 10³-vertex polygon ~4×10¹⁵ — inside `i64`, with `i128` carried for headroom.
Orientation predicates are exact `i128` determinants. **No adaptive-precision filter
(Shewchuk 1997) is needed because we never leave the integers.**

- *Cheap wrong way:* `ST_Contains` in double precision.
- *Silent error:* a footprint straddling a lot line by 3 cm goes to lot A on x86, lot B on
  ARM, and a third answer after a GEOS point release. In 40,000 loans that is a handful of
  silently different answers per rerun with no detection mechanism.
- *What exact buys:* byte-identity across platforms and decades by construction.

---

## 5. The variable model

Canonical total order `≺` on all features: `(source_rank, source_native_id_bytes)`, with
`source_rank` from a versioned table. Variable order, diagram order, report order and
tie-breaks all derive from `≺`. **No hash-map iteration in any order-sensitive path.**

**Latent layer.** Parcels `P` are given (~25/tile); parcel geometry is a versioned candidate
substrate within its source scope, not metaphysical ground truth, and overlapping legal
hierarchies require typed crosswalks. Attributes go through ρ. Latent buildings
`B = {b₁…b_K}`, `K = Σ_p NUMBLDGS(p)` where
present, else per-component max footprint count across sources, plus `⌈0.2K⌉` slack slots
under an `atmost`. `K ≈ 60–80`.

```
X_f  ∈ B ∪ {⊥}       observed footprint → slot   (Overture, FEMA, MS)   ~180
Y_q  ∈ B ∪ {⊥}       POI → slot                                          ~40
Pb_b ∈ P ∪ {∅}       slot → parcel                                       ~80
A_b  ∈ [a_lo,a_hi]   integer footprint area, whole sq ft                 ~80
Fl_b ∈ [1,120]       floor count                                         ~80
Lo_ℓ,Hi_ℓ ∈ ℤ        address-range endpoints per lot per block face      ~50
Coll ⊆ P             collateral parcel set        (ROBDD set variable)
QB   ⊆ B             collateral building set      (ROBDD set variable)
```

`n_fd ≈ 260` finite-domain, `n_int ≈ 210` integer. `d_max = K ≈ 80` before geometric
filtering; `d_typ ≈ 8` after.

> **CURRENT STATUS — PROPOSED / UNMEASURED.** The variable vocabulary remains the design
> under review, but these counts are not a measured sizing basis. Appendices F and G replace
> tile-wide feature arithmetic with component-wise sizing and observe parcel-star components
> up to 71 variables in NYC.

### 5.1 Symmetry must be broken completely and soundly

Slots `b₁…b_K` are interchangeable. `K!` symmetry **destroys model counting outright** —
every solution appears `K!` times. Two mechanisms channelled together (Cheng, Choi, Lee &
Wu 1999):

- **Representative encoding** for canonicity — a latent building is identified with the
  `≺`-least observation in its cluster. No anonymous slots, weak propagation.
- **Slot encoding** for the strong global propagators, with **value precedence**
  (`precede`; Law & Lee 2004; Walsh 2006) breaking value interchangeability completely at
  GAC in O(nd).

- *Cheap wrong way:* cluster, then sort clusters and call them 1..k.
- *Silent error:* the count is wrong by `K!/orbit size`, so "3 candidates" and "3 million
  relabellings of 1 candidate" are indistinguishable and every ambiguity measure is noise.

---

## 6. The consistency ladder, with arithmetic

> **CURRENT STATUS — ORIGINAL SIZING FALSIFIED; OPERATOR ORDER PROPOSED.** The 6–20-variable
> component estimate, 0.5 s tile budget, and tile-wide cost arithmetic below must not be
> quoted as current. Appendices B and C falsify the original decomposition and work-unit
> assumptions; Appendix F restores decomposition under the canonical geometric predicate
> but finds parcel-star components up to 71 variables; Appendix G requires a component-wise
> cost model with explicit halo reconciliation. No end-to-end solver runtime has been
> measured. The ladder below is therefore an architectural proposal, not a benchmark.

**The ceiling is not a consistency level.** Régin's `alldifferent` GAC computes strongly
connected components of the value graph (Tarjan 1972) as an intrinsic step — **those SCCs
*are* the tile's decomposition, handed to us free.** No separate tree-decomposition
heuristic. Typical component after geometric filtering: **6–20 variables, d ≤ 8**, tail to
~40 on a dense assemblage.

At that size, **exact compilation of the entire solution set is cheaper than path
consistency on the tile, and subsumes k-consistency for every k simultaneously.** The
crossover is at **k = 3**.

```
NC  →  AC-2001 + GAC on globals   ≈ 10 ms   tile-wide
    →  SAC                        ≈ 0.3 s   tile-wide   ← the level that earns its keep
    →  decompose                  free      from Régin/Tarjan
    →  exact MDD/SDD per component ≈ 0.2 s  ← subsumes all k-consistency at once
    →  PC on components           ≈ 50 ms   ← explanation artifact, NOT pruning
```

**Tile budget ≈ 0.5 s.** A spatial join is ≈ 1 ms.

> **We spend 500×, and that is the entire commercial thesis.**

At 10⁶ tiles: ~140 CPU-hours, embarrassingly parallel — **a few hundred dollars of compute
for a national pass.**

### 6.1 What each level buys that the level below cannot

**AC over pairwise `≠` cannot see Hall sets.** Six MS GlobalML footprints, five
geometrically admissible slots. Pairwise disequality with AC finds nothing — every value
still has support. Régin's GAC finds the wipeout immediately, because Hall's theorem (1935)
violations are exactly what the SCC decomposition detects. **This is a proof that MS
over-segmented a roof ridge, emitted for free.**

**SAC buys eliminations requiring an assignment plus a numeric constraint.** Assume the
collateral is lot A. Propagate. The knapsack propagator on `Σ A_b · Fl_b` cannot reach the
asserted 214,300 sf even using every compatible footprint at maximum plausible floor count
→ wipeout → **lot A eliminated with no threshold and no search.** Plain AC never sees this
because the sum is violated only *in combination with* the assignment.

**PC and SAC are incomparable as domain filters** (Debruyne & Bessière, JAIR 14, 2001; the
lattice is `AC ≺ RPC ≺ maxRPC ≺ SAC` with PC orthogonal as a *relation*-filtering
consistency). PC's distinctive product is the pairwise relation itself — *"if the
collateral is lot A then the FEMA structure must be `f3`"* — which SAC can never represent.

**Therefore PC is demoted from pruning to explanation.** Post-decomposition, post-SAC,
PC-2001 on components approximates Montanari's (1974) **minimal network**: the network
whose binary relations are exactly the projections of the solution set. The human-readable
pairwise summary of the residual. Run for the report, not the answer.

**Strong k-consistency for k ≥ 4 is affordable on components and worthless there** — for
the same ~5 s you can compile the component exactly and get every k at once plus the count
plus the backbone. **Ranked to zero.**

**Freuder's 1982 theorem gives a per-tile certificate.** If a component's constraint graph
has width `w` under the canonical ordering and the network is strongly `(w+1)`-consistent,
search is backtrack-free — the propagation fixpoint *is* the solution set. Compute `w` per
component and report it. A tile carries the line *"solved backtrack-free at width 2"*,
which is a mathematical statement about that tile, not a QA note.

---

## 7. The global constraint catalogue

Domain rules that look like generic pairwise checks are instances of **named global
constraints with polynomial domain-consistent propagators.** Hand-coding them as pairwise
checks discards decades of work and prunes far worse.

| Domain rule | Global constraint | Algorithm / authority |
|---|---|---|
| Within-source exclusivity (two Overture buildings are never the same building) | `alldifferent` / `alldifferent_except_0` | Régin 1994, via Hall's theorem + Tarjan SCC |
| Cardinality priors (`NUMBLDGS`, source coverage rates) | `gcc` (global cardinality) | flow-based |
| Distinct building count | `nvalue` / `atmost_nvalue` | — |
| Additive area — **not** "sum with tolerance" | `knapsack` / `bin_packing` | subset-sum DP with dedicated propagator |
| Parcels do not overlap | `diffn` / `geost` | Beldiceanu et al. |
| Address along a block face | `disjunctive` scheduling on the house-number axis | — |
| **Address string parsing** | `regular` | Pesant, CP 2004 — GAC by DFA unfolding, O(n·\|Q\|·\|Σ\|) |
| Temporal feasibility | Allen interval algebra / STP | Allen 1983; ORD-Horn tractable subclass (Nebel & Bürckert 1995); STP by Floyd–Warshall (Dechter, Meiri & Pearl 1991) |
| Ownership equivalence | `nvalue`, `among`, equivalence constraints | — |
| **Identifier namespaces** | functional dependencies + **congruence closure** | Nelson–Oppen 1979; union-find with proof forests |
| Containment (building on parcel, POI in building) | `inverse`, channelling, b-matching | — |
| Set variables (assemblages, `Coll`, `QB`) | ROBDD set domains | — |
| Slot symmetry / ordering | `precede`, `lex_chain` | Law & Lee 2004; Walsh 2006 |

### 7.1 Three that deserve calling out

**`regular` puts the address grammar inside the solver.** The naive way is libpostal — a
CRF, therefore statistical, therefore nondeterministic across versions and uninterpretable
— which **picks one parse**. Silent error: `"199 First Avenue, Unit 3B, a/k/a 355 East 12th
Street"` gets one parse, the `a/k/a` is discarded, and the true answer is destroyed before
the solver runs. With `regular` over a declared versioned token grammar, **all parses stay
alive as a domain** and the other constraints kill the wrong ones. Alternation handles
`a/k/a` natively. **This removes the last statistical component from the decision path.**

**Allen's interval algebra finds demolitions.** MS footprint from 2021 imagery, FEMA
structure from 2019, parcel `YEARBUILT` 2020. A spatial join merges all three into one
building. The temporal network **proves** the 2019 FEMA record cannot denote the same
physical structure — so the tile contains a demolition-and-rebuild event, meaning **the
collateral described in the 2019 offering document no longer exists.** A five-alarm CMBS
finding, falling out of a 1983 paper. *Cheap wrong way:* `WHERE year_built <= 2019` — it
filters rows instead of detecting events, so the rebuild is invisible.

> **CURRENT STATUS — OPEN.** The evidence compiler now preserves valid-time intervals and
> refuses to turn them into timeless hard or soft constraints. No Allen/STP network or
> query-as-of composition domain is implemented, so no demolition/rebuild proof is a
> current Canon capability.

**Congruence closure makes identity conflicts proofs.** Maintain equivalence classes of
entity variables and identifier literals; every union records the named evidence
responsible; **every attempted union with an incompatible namespace id produces a conflict
proof.** Inverse-Ackermann per operation. *Cheap wrong way:* coalesce ids after choosing a
parcel — the conflict is discovered too late or silently overwritten. **A conflict is a
proof, not an exception log.**

---

## 8. Explanations as a byproduct

Three candidate paradigms with different cost profiles:

**(a) ATMS** (de Kleer 1986; + GDE, de Kleer & Williams 1987). Every derived datum carries
a label: the minimal environments under which it holds. Explanation *is* the data
structure. **Honest cost:** labels are antichains and can blow up exponentially — with ~200
source records as assumptions per tile this is a real risk. **Do not run a full ATMS
eagerly.**

**(b) QuickXplain** (Junker, AAAI 2004). Preferred minimal explanation on demand in
**O(k log(n/k))** consistency checks. At n ≈ 60 tile constraints, k ≈ 3: `3·log₂(20) ≈ 13`
solver calls × ~10 ms = **~130 ms per explanation, paid only when an operator clicks.**
Fully deterministic given a fixed constraint order, which the source-reliability ordering
supplies. **This is the right engineering answer.**

**(c) Lazy Clause Generation** (Ohrimenko, Stuckey & Codish 2009) — propagators explain
themselves in clauses; the resolution derivation is the proof. Certified with **VeriPB**
(Gocht, McCreesh & Nordström, CP 2022), which can certify global-constraint propagation
*and* symmetry breaking, which a naive DRAT log cannot.

### 8.1 The committed architecture

- **Answer layer:** compile to a representation selected for the required operations. A
  canonical reduced form under a frozen order/vtree can remove semantic representation
  variance; compilation may still search, and byte identity additionally depends on a
  deterministic implementation and frozen serializer. General d-DNNF does not provide
  canonicity by itself.
- **Explanation layer:** QuickXplain on demand, ordered by declared source reliability.
  Artifact is a minimal set of named source records: *"lots 1012920026 and 1012920001 are
  separated by exactly {FEMA `f3` SQMETERS = 3,240; MapPLUTO `NUMBLDGS` = 2; the First
  Avenue block-face anchor at 195}."* Templates to prose because every constraint carries
  provenance by construction.
- **Certificate layer:** VeriPB proof log for the full run, **independently checkable by a
  third party who does not trust our code.**

### 8.2 The determinism precondition people skip

**Confluence, determinism, soundness, and completion are separate contracts.** For a fixed
initial store, fair iteration of monotone contracting propagators to quiescence on a finite
lattice yields the order-independent closure relevant here. Monotonicity alone does not
make a propagator sound, and a deterministic function can still be unsound. Randomized
rounding or sampling destroys reproducibility unless fully frozen and may also destroy
soundness/monotonicity. An early work limit does **not** automatically make each propagator
non-monotone; it means closure may be incomplete, so the artifact must say so instead of
claiming the fixpoint theorem.

Where we search rather than compile (components exceeding the width budget), byte-identical
*proofs* additionally require: canonical branching order from `≺`; restarts driven by a
deterministic counter, never wall clock; no PRNG without a fixed seed; no propagator reading
external mutable state. **A single `HashMap` iteration in a propagator silently destroys the
guarantee.**

---

## 9. Solver-native artifacts — the actual product

Compiling to **d-DNNF / SDD / reduced MDD** (Darwiche 2001, 2011; Darwiche & Marquis 2002;
Andersen, Hadžić, Hooker & Tiedemann 2007; Bergman, Cire, van Hoeve & Hooker 2016) makes
all of the following linear or polynomial in diagram size.

| Artifact | Computation | Operator product |
|---|---|---|
| **Backbone** — values in every solution | one traversal | *"Regardless of how the ambiguity resolves, this loan touches BBL 1012920026, GERS `08f2a3…`, and total collateral GLA ≥ 412,000 sf."* **Lets a downstream system act on partial resolution.** |
| **Exact model count** | one bottom-up pass over a completed deterministic/decomposable representation | A *calibration-free* ambiguity measure. Not a confidence score — a count. A completed unsaturated 1 = decided, 3 = three named alternatives, and 0 = proof of source defect; fallback placeholders and saturated lower bounds are different claim classes. |
| **Residual enumeration** | polynomial delay for supported compiled/matching classes | The full alternative set when materialization is within budget. Ryser (1963) gives O(2ⁿn) exact matching counts — practical for small proven factors such as n=12, not for a raw n=200 component. *#P-complete in general (Valiant 1979); tractability must come from measured decomposition or compiled width, never tile row count alone.* |
| **MUS** — minimal blame | QuickXplain | *"These five sources cannot all be right, here is the smallest set that proves it, ordered so the least-trusted source is named first."* |
| **MCS** — minimal repair | hitting sets of MUSes (Reiter 1987); enumeration via CAMUS (Liffiton & Sakallah 2008) or MARCO (Liffiton et al. 2016) | *"Retract either {FEMA `f3` SQMETERS} or {MapPLUTO `NUMBLDGS`} and the tile becomes consistent. Nothing smaller works."* **A repair recommendation, not an error message.** |
| **Counterfactual separation power** | exact count reduction under each precisely stated hypothetical fact | *"If the certificate-of-occupancy date has value `d`, this exact fraction of the residual is eliminated."* This is exact realized/counterfactual reduction, not yet expected value of information. |
| **Minimal network** (Montanari 1974) | PC on the residual component | *"If lot A then FEMA `f3`; if lot B then FEMA `f7` and the POI is a tenant not the owner."* |
| **Certified refinement** | entailment plus non-emptiness between diagrams over the same declared universe and semantics; polytime on SDDs sharing a vtree | *"Every 2027 model was allowed in 2026, and at least one 2027 model remains."* An empty successor is a typed contradiction, not a vacuous success. |

### 9.1 The committed ranking

**Contractual output, build first: backbone completeness plus a scoped count and its
exactness.** Exact backbone/count are nearly free once a suitable compiler exists; before
then, count completeness, saturation, and typed budget fallback must remain distinct. A
fallback placeholder is not zero; a completed unsaturated zero is a proof of conflict; a
completed saturated value is a declared lower bound, not an exact `u64` count.
This converts abstention from a failure into a deliverable without making the SLA depend on
an unchosen representation.

**Highest-margin single artifact: the ordered MCS lattice.** Backbone can be *approximated*
— a competitor with a good probabilistic model can produce a "high confidence subset" that
is usually right, and usually-right sells. **MCS has no approximation.** There is no
statistical proxy for "the minimal set of retractions that restores consistency." It is
also the only artifact with a buyer *other than* the person who asked the question — the
data vendor, the trustee, the risk committee — and **the only one that improves the input
corpus rather than consuming it, so it compounds.**

**Compounding moat: a value-of-information foundation.** Exact counting makes separation
under each hypothetical observation exact. Turning that into expected VoI and procurement
optimisation additionally requires a calibrated distribution over possible observations,
acquisition cost, and decision utility; those must never be inferred from count reduction
alone. This is the thing
that makes the corpus asymmetric over three years, and it directly answers "which dataset
do we buy next" from real residuals rather than intuition.

**Regulatory: certified refinement.** In CMBS specifically, *"we can hand the trustee a
proof that every surviving restatement model was previously allowed, and that the new set
is nonempty"* is worth more than it sounds. If the successor is empty, the deliverable is
a contradiction certificate instead—not a vacuously true refinement claim.

---

## 10. Where the frame breaks — answered with a theorem

**Semiring-based CSP** (Bistarelli, Montanari & Rossi, JACM 44(2), 1997):

> **Soft constraint propagation is confluent and reaches a unique fixpoint iff the
> semiring's combination operator × is idempotent (a × a = a).**

- **Fuzzy / possibilistic** — `⟨[0,1], max, min⟩`. `min` is idempotent. **Confluent. Safe.**
- **Weighted** — `⟨ℕ∪{∞}, min, +⟩`. `+` is not idempotent. Soft arc consistency (Cooper &
  Schiex 2004; Larrosa & Schiex 2004) requires equivalence-preserving transformations and
  **the fixpoint depends on the order they are applied.**

**So "can we just add reliability weights?" is answered no, and here is the paper.**

### 10.1 Where softness lives instead — three places, none of them the solver

1. **In ρ** — declared, versioned, falsifiable bands (§3). Gross-vs-NRA is two hard
   relations plus a band `[0.78, 0.95]` for office, with a version number and a citation.
2. **In presentation ranking** — genuine preferences applied to the **already-enumerated
   finite residual**, as a sort with canonical total order and tie-breaking. Sorting a
   finite enumerated set is confluent by construction. **The solver never sees the
   preference.**
3. **Reliability, which is not a weight** — it sets the *width* of a source's ρ band and
   supplies the *preference order* handed to QuickXplain. **Reliability never weights a
   decision. It widens a band and orders a report.**

> **Rule: preferences rank; constraints prune. Never mix.**

### 10.2 The claim-class stratification

If valued/semiring CSP *is* used, it remains deterministic given exact costs and
tie-breaks — but **adding a soft constraint can change the optimum**, so the "knowledge only
tightens" guarantee does **not** extend to preferred answers. Output must therefore
separate:

```
HARD_FORCED     true in every hard-feasible model
SOFT_PREFERRED  true in every minimum-cost model under declared policy
SOFT_RANKED     ranked alternatives, not facts
```

**Never promote `SOFT_PREFERRED` as a canonical identity fact** unless the product contract
explicitly allows policy-dependent identity.

> **Softness does not destroy determinism. It destroys the right to call the optimum "the
> truth." Keep those separate and the architecture remains honest.**

### 10.3 When hard constraints conflict

1. Emit the MUS or a small irreducible conflict.
2. Compute MCS / minimum-cost repair **as diagnosis only**.
3. **Do not return a resolved identity.**

If the conflict involves constraints that should have been soft, **the fix is not to weaken
the solver. The fix is to reclassify the evidence contract.**

**Fallback is not fuzzy matching. It is a lower claim class:** hard residual unresolved;
soft ranking available; minimal repairs available; human review target available.

---

## 11. What this supersedes in the existing geo epic

| Bead | Status under this plan |
|---|---|
| bd-2cbs entity-level model | **Retained and strengthened** — levels become typed variables and channelling constraints |
| bd-16r1 geometry typed value | **Retained** — the per-tile integer frame is exactly this, now with an arithmetic bound |
| bd-3nc7 predicate regime | **Resolved** — integers in a tile-local frame; no adaptive-precision filter needed |
| bd-15ba exact predicates | **Demoted** — Shewchuk becomes a fallback, not the bar |
| bd-2zdz assemblage subset selection | **Superseded** — becomes `knapsack`/`bin_packing` + set variables, not bespoke interval enumeration |
| bd-786w coverage abstention | **Superseded** — abstention is the residual model set; reason codes become MUS/MCS output |
| bd-272d attribute anchoring | **Retained as a constraint**, demoted from decider (see §2.1) |
| bd-1a12 geocode plausibility | **Retained** — becomes ρ radius selection plus an empty-model-set proof |
| bd-1uje / bd-3d8p / bd-1c96 / bd-3h2p / bd-3ul7 ambition lane | **Mostly superseded** — assignment and clustering become global constraints with exact propagators; revisit each against §7 |
| bd-101v visual evidence card | **Retained and easier** — minimal network + MUS are the card's content |
| bd-tccn worked-case corpus | **Retained, now the validation harness** for the propagator library |
| bd-35qg address-set source | **Elevated** — the red team's central recommendation: much of the machinery exists to compensate for a missing address-point layer |

---

## 12. The acquisition finding the red team surfaced

> *"Most of the parse forest exists to compensate for a missing data source. You do not
> have an address-point layer, and you need one."*

NYC PAD / Geosupport contains every legal address per lot including all frontages and
a/k/a's, encodes Queens grid semantics correctly, and already knows that 9 West Fordham
Road and 2167 Grand Concourse are the same lot. Deterministic, integer-keyed, explainable,
no model in it, maintained by the jurisdiction that *defines* the answer.

With it, several hard problems collapse to lookup rather than enumeration. Outside NYC the
analogue is the county address-point file or the National Address Database. **Imperfect
coverage is fine — imperfect coverage produces honest abstentions.**

---

## 13. Cost model and the commercial thesis

> **2026-09-01:** the commercial thesis is restated in §18.2 as three deliverables with named buyers; the cost figures below are `CUT` (§18.3).
>
> **CURRENT STATUS — FALSIFIED / OPEN.** The numerical model in this section is retained as
> the original commercial hypothesis. Appendices C and G falsify its work-unit sizing, and
> Appendix F changes the computational unit from the whole tile to geometric components.
> Until E4 records component compilation, propagation, halo-reconciliation, and fallback
> costs, neither 0.5 s/tile nor 140 CPU-hours nor “a few hundred dollars” is an admissible
> product or planning claim.

```
per tile        ≈ 0.5 s        vs ≈ 1 ms for a spatial join      → 500×
national pass   ≈ 140 CPU-hours at 10⁶ tiles, embarrassingly parallel
                ≈ a few hundred dollars of compute
```

**The moat is not the data.** Overture, FEMA, Microsoft footprints and county parcel data
are public. The moat is being willing to spend 500× per tile running exact combinatorial
methods, because the tile bounds the problem to ~200 nodes and turns globally intractable
techniques into free ones.

Nobody in commercial real estate knows these techniques exist, and nobody who knows these
techniques has looked at a rent roll.

---

## 14. Open questions and risks

These are the current gates for the main review. Earlier questions that measurements have
partially answered are narrowed here rather than silently removed.

1. **E4 — composition capability.** Can the actual joint constraint set recover honest
   parcel/building residuals on the six worked cases and the labeled multi-parcel
   population? Record backbone accuracy, residual sizes, false merges, abstentions, and
   component costs. Point re-ranking is not a substitute for this test.
2. **E5 — genericity and evidence tiers.** Does the same architecture run in a non-NYC
   county without a special code path, and what coverage/precision/abstention curve results
   as address sets, footprints, document evidence, and attributes disappear?
3. **Truth-instrument cleanup.** Rebuild or independently adjudicate the Gate V2 truth set
   with lender/party evidence and a typed condo unit↔billing lot↔building crosswalk before
   promoting any precision number to a release claim.
4. **Component-wise performance and fallback.** Measure propagation, exact compilation,
   model counting, explanation, and halo reconciliation on the observed component
   distribution, especially parcel stars near 71 variables. Define the deterministic
   search or decomposition fallback and its claim class before setting a budget.
5. **ρ contracts and calibration.** For every admitted source, distinguish a logically
   sound relaxation from an empirically high-coverage band. Name the population, error
   characterization, owner, version, and falsification procedure. The illustrative
   0.78–0.95 office NRA band is not yet admissible evidence.
6. **Solver and compiler feasibility in Rust.** Identify the minimum useful subset of §7,
   verify maintained implementations or scope new work, and test whether reduced MDD,
   SDD, or another representation supports the required count/backbone/refinement
   operations under canonical ordering.
7. **Certificate practicality.** Verify what VeriPB can certify for the chosen encodings
   and global propagators, then specify proof granularity, size budgets, retention, and
   independent-check workflows. Do not promise whole-run certification before this test.
8. **Deterministic geometry contract.** Validate the tile-local integer projection,
   quantization error, overflow bounds, boundary semantics, and cross-platform byte parity
   against the actual ingest/projection path.
9. **Set representation and BYOP boundary.** Test `Coll`/`QB` representation at realistic
   assemblage sizes and decide which compiled artifacts may contain client geometry, which
   can be cached, and which may leave the client environment.
10. **Citation and theorem audit.** Independently verify every load-bearing theorem,
    complexity, attribution, and claimed proof-system capability before it appears in an
    external argument or implementation acceptance criterion.
11. **Scope discipline.** Every item above is now `IN`, `DEFERRED`, or `CUT` under §18.3;
    items 6 and 7 are `DEFERRED` with triggers, item 4 is re-aimed at component costs
    measured with evidence admitted (§19.5, D1), and item 3 is stage D0.

---

## 15. Provenance, and what is NOT yet verified

This plan was produced by an adversarial multi-model design session on 2026-08-14/15:

1. A cross-domain technique search (`WIZARD_IDEAS_CC.md`, `WIZARD_IDEAS_COD.md`)
2. An identifier-authority ambition round (`WIZARD_AMBITION_COD.md`)
3. Cross-model adversarial scoring (`WIZARD_SCORES_*.md`)
4. A red team that **destroyed two prior architectures** (`REDTEAM_CC.md`)
5. This constraint-object round (`CSP_CC.md`, `CSP_COD.md`), where two models converged
   independently on the same formal object

Convergence between two model families is useful hypothesis-generation evidence, but it is
not independent empirical validation: both models can inherit the same literature priors,
prompt framing, and blind spots. The strongest evidence is executable counterexamples,
fresh measurements with declared denominators, and held-out truth gates. Model convergence
earns an experiment; it does not pass one.

### What is NOT verified

- **Most of the ~50 academic citations above have not been independently checked.** The
  2026-08-27 audit checked the narrow canonicity and fixpoint corrections against primary
  Bryant/Darwiche/Cousot sources; it did not validate the remaining authors, dates,
  complexities, or proof-system claims. **Verify each load-bearing claim before citing it
  externally or committing engineering to it.**
- All solver runtime and national-cost numbers (0.5 s/tile, 140 CPU-hours, propagation and
  compilation costs) are **estimates from the analysis, not measurements.** Appendices
  B–G measure work-unit and component distributions, and they falsify the original sizing;
  they do not supply an end-to-end runtime benchmark.
- The information-theoretic checksum argument in §2.1 is internally consistent but its
  inputs (400× size range, 10–20% NRA gap) are drawn from a small measured sample.
- Whether usable Rust implementations exist for the named global constraints is **unknown**.

### Session lesson encoded here

Three claims during this session came from model prose rather than returned values, and all
three were wrong. **Take literal values, record the query, verify citations before relying
on them.** This document is a design to be validated, not a set of established facts.

---

## 16. The resolution task, operationally

Added 2026-08-16, operator-approved, after the Appendix K review exposed the gap: the plan
specified the mathematics (§4–§10) and the admission discipline (§3) but never the
operational middle layer. This section closes it.

### 16.1 The query

Target input: one typed `GeoQuestion` naming the subject, bounded geography, requested
entity grains, explicit query-as-of domain where time may constrain the result, claim
classes, abstention policy, and resource budget. It is paired with a regional evidence
inventory and resolution profile. The question names evidence classes and desired grains,
not vendors and not a mandatory parcel source.

The current proving-ground adapter supplies one CMBS property record—its address string(s)
(possibly multi-address, ranges, a/k/a), geocode(s) with accuracy tier, asserted attributes
(SF, units, year built from Annex A / the loan documents), and loan identity for document
evidence. Current composition v0 has explicit `canon_geo_composition_profile.v0` selection
levels: omitted/default `parcel` preserves the non-empty parcel universe requirement, and
explicit `building` permits an empty parcel universe while enumerating building-level
residuals without consulting a legacy parcel oracle. It does not create a parcel-grain
answer without a parcel source.

For the current CMBS profile, output is the **collateral parcel set** `Coll` and **building
set** `QB`, delivered in the
§10.2 claim classes — `HARD_FORCED` facts when the backbone is complete, a residual count
with entity-selection scope plus independent completeness and saturation metadata,
materialized residual models only inside the declared presentation budget, `SOFT_RANKED`
alternatives where policy allows, or a typed fallback/refusal. A proven empty residual is
kept distinct from explanation completeness: an oversized conflict may carry a
deterministic constraint superset when minimal-core reduction exceeds its own budget. A residual
of size >1 is a deliverable, not a failure (§9.1). Case 6's shape is normative: parcel
singleton, building doubleton, both stated. **The answer is the best-supported entity at
each level; any ledger key (BBL, BIN) is an alias projection of that entity, and an
unavailable ledger form never voids a resolved entity (L.5).** Refutation of the input
itself ("the asserted address is nowhere in this tile") is an abstention that triggers
reacquisition—re-geocode and retry—not a terminal failure. Other profiles are not yet given
exact composition semantics by v0; unsupported parcel, site, or address grains must not
erase a supported parcel/building result.

### 16.2 Candidate enumeration

Candidates are never proposed by a channel (§2: there is no proposer). The generic
candidate universe is every profile-permitted entity in the bounded tile/halo, separated
by entity level and relation type. Under the default parcel profile, current composition v0
instantiates this as all parcels in the work unit plus building/parcel incidences. Under
explicit building selection, it uses the declared building universe directly and permits no
parcel side variables. Geometry may add typed compatibility constraints inside a declared
stratum; the solver then decomposes the actual variable/constraint incidence graph rather
than assuming a forest from geometry alone.
For the current CMBS profile, `Coll` candidates are subsets of component parcels, pruned by
hard constraints—the knapsack over asserted SF, adjacency, ownership-permits (never
forbids), and document-asserted BBL sets. Enumerate within components (measured sizes: 2–5
typical, parcel-stars to ~71 per Appendix F); the residual is whatever survives
propagation. A parcel-free profile must construct building/site/address components without
injecting a fake parcel or treating missing parcels as negative evidence.

### 16.3 The evidence inventory

Every evidence class, its landed NYC instance, its generic analogue, its ρ, what it feeds,
and its measured state. **The class is the architecture; the instance is data onboarding
(Appendix A.6). Canon core never special-cases an instance.** Rows marked UNMEASURED are
open work, not established capability.

| # | Evidence class | NYC instance | Generic analogue | ρ — sound reading | Feeds | State |
|---|---|---|---|---|---|---|
| 1 | Geocode point + tier | `WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` | any geocoder | proposed disc of tier-dependent radius; never a location, only a bound. The E4 extension contains one reachable-truth falsification, so the current GeoDISC contract is diagnostic pending recalibration. | counterfactual position separation; hard constraint only after an admissible contract | MEASURED separation and falsification; hard admission `OPEN` |
| 2 | Address strings (query side) | `PROPERTY_ADDRESS` + parsed fields | any | existential membership — "some member of `Coll` fronts street S near number N"; parses held as a domain via `regular` (§7.1); never exclusion | `Lo/Hi` range vars, membership | MEASURED: 28.89% exact-fire; string normalization is representation-bound (K.2) |
| 3 | Address sets (lot side) | **LANDED 2026-08-16**: `NYC_DCP_PAD_ADDRESS_HOT` (1.32M), `_PAD_BBL_HOT` (874K), `_PAD_SND_HOT` (121K street names) + EXT/meta | county address points, National Address Database, OpenAddresses | the lot's full legal address set; membership tests against it | address-set membership; direct address→BBL+BIN lookup | VERIFIED on acceptance probes: Crosby/Broadway both frontages (Case 5); 241–249 W 74 range→BBL 1011660007 (retry case, no geocoder); Queens hyphenate 130-50 146 St→3 BINs (F.4's disagreement parcel confirmed) |
| 4 | Parcel geometry | `NYC_DCP_MAPPLUTO_HOT` 26v1 | county parcels / Regrid | survey substrate; exact integer predicates (§4) | candidate universe, area-majority anchor | MEASURED (D/F) |
| 5 | Footprints, multi-source | NYC footprints; FEMA structures; MS GlobalML (NY landed); Overture buildings (landed and measured in F.6's bounded six-stratum sample) | same, national | geometric-area majority inside an interior-disjoint parcel stratum; overlapping legal hierarchies use typed crosswalks; within-source `alldifferent`; cross-source counts via `gcc` | `X_f`, `Pb_b` slots | Mixed-contract forest retained (F); NYC+Overture predicate-incidence measured in F.6; latent-building reconciliation and canonical multi-source solver rerun `OPEN`; 55% retained count agreement (F.4) |
| 6 | Asserted attributes | Annex A / `PROPERTY_MART` SF, units, year | every deal tape | exact integer bands only when semantic id, unit, value origin, and calibration basis agree; the illustrative office NRA/gross band is diagnostic, not admitted | `A_b`, `Fl_b`, knapsack | LANDED; typed evidence compiler implemented; population calibration/joint test `OPEN` |
| 7 | Parcel attributes | `BLDGAREA`, `NUMBLDGS`, units | assessor rolls | observations to check, never denominators (F) | `gcc` counts, area bands | PARTIALLY MEASURED (F.4) |
| 8 | Document evidence | `NYC_ACRIS_*` external tables | county recorder / title plants | recorded collateral BBL sets bound by amount+date+lender with contamination filters (H) | direct `Coll` evidence; also ground truth | MEASURED as truth instrument (H); unused as solver evidence |
| 9 | Imagery / elevation observers | none landed; verified catalog in J (NYS/NYC ortho first, 3DEP, NAIP, NOAA event) | national per J | frozen-weight observers emit typed counts/outlines/floors with characterized regional error (A.2–A.3) | `gcc` checks, an own footprint source, change events | UNMEASURED |
| 10 | POI / tenant | not landed | — | existential presence; tenant ≠ owner | `Y_q` | UNMEASURED |
| 11 | Ownership | `OWNERNAME` | — | permits assemblage, never forbids (§3) | `Coll` permitting | ~0 bits alone by design |
| 12 | Temporal / vintage | per-source dates, `YEARBUILT`, document dates, imagery vintage | — | closed whole-day valid-time intervals; diagnostic until an explicit query-as-of domain and Allen/STP solver exist (§7.1) | future demolition/rebuild events | Interval admission implemented; temporal inference `OPEN` |

Per §2.1, **every row is worth a few bits at most, by design.** The architecture's claim
is that joint propagation measures the *conditional* residual reduction after all prior
evidence—not that nominal source bits add independently. Redundant constraints contribute
zero additional reduction; shared-lineage errors remain shared risks. No row is a decider;
no row should ever be tested as one.

## 17. Evaluation protocol

Added 2026-08-16, operator-approved. The standing rule this plan lacked, stated first:

> **Constraints are never evaluated as unary accept/refute rules. The unit of measurement
> is the joint residual over the candidate set.** Individual-signal saturation and
> false-refutation are expected consequences of §2.1's few-bits premise — measuring them
> (Appendix K) characterizes inputs; it neither confirms nor refutes the architecture.

The dispositive ladder — each stage gates the next, all numbers from structured results
with the query recorded, denominators predeclared, scored against the H.6 baselines on the
coverage/precision plane:

- **E1 — Failure taxonomy.** Classify the 79 labeled Gate V2 failures by cause:
  adjacent-lot near-miss (with distance to true lot), gross geocode error (W 49th class),
  assemblage scoring artifact, condo representation residue, residual truth contamination.
  Bounds achievable headroom per evidence class before anything is built.
- **E2 — Attribute-channel reconciliation.** Join Annex A asserted SF/units/year to the
  labeled set; measure band-consistency of asserted SF against the PIP lot vs the true
  lot. First exercise of inventory row 6.
- **E3 — Pairwise candidate test (the solver stand-in).** For each labeled failure, score
  the true lot against the PIP lot under all landed inventory rows jointly. The headroom
  number is: how often does joint evidence rank the true lot strictly above the wrong
  one? This is the architecture's claim in measurable form.
- **E4 — Joint mini-propagation.** Run the actual constraint set (§5–§7) over the six
  corpus cases (I) and the labeled set: residual sizes, backbone accuracy, abstention
  honesty, per-component cost against Appendix G's sizing.
- **E5 — Genericity gate.** Repeat E1–E4 in one non-NYC county using only generic
  instances (county parcels, NAD/county address points, NAIP or state ortho). No
  NYC-specific code path may be load-bearing. **Operator doctrine (2026-08-16): evidence
  availability is a property of the geography, not the architecture.** Most geographies
  will not have a PAD-quality address-set layer; some will lack footprints, document
  records, or attributes. The resolver must run with *any subset* of §16.3's rows present,
  do the best the local evidence supports, and **abstain where it cannot** — never degrade
  precision to preserve coverage. E5's deliverable is therefore not one number but the
  **evidence-tier → coverage/precision curve**: the operating point at NYC's full stack,
  at a typical county stack (parcels + NAD/county points + FEMA + NAIP), and at a
  minimal stack — with abstention rates reported per tier as first-class output. A
  geography with thin evidence yields honest abstentions, not silent guesses; that is the
  same band-versus-threshold rule (§3.2) applied to source availability itself.

  **2026-08-31 preflight, not gate execution.** Franklin County, Ohio (`39049`)
  has a nonzero generic thin stack around real collateral subjects. File-exact
  query `01c6c151-0821-a0dc-006c-c703088daaba` bounded 151 geocoded properties
  to 114 r8 centers and 585 distinct center+k1 work cells; all four required
  availability rows passed their guards: FEMA structures 160,773 features in
  582 cells, Microsoft footprints 168,778 in 581, Overture addresses 310,650 in
  581, and Overture buildings 203,367 in 584. These are H3 blocking counts, not
  independent information, parcel incidence, or candidate recall. This preflight
  is immutable: it accurately recorded that the parcel layer was absent on
  2026-08-31 rather than being rewritten after the landing.

  **2026-09-01 parcel-backed successor, still not gate execution.** The Franklin
  Auditor source now exposes 494,704 release-pinned parcel rows, original EPSG:3735
  WKB plus hashes, H3 feature coverage, and one non-degraded transform receipt.
  Admission retains 494,043 rows. Over the current build's 151 geocoded property
  subjects, the complete r8 feature-coverage block contains 54,344 candidate pairs;
  the Snowflake reference predicate reaches 147 properties (146 unique, one
  two-parcel) and misses four. Each miss had candidates and lay 3.006–22.221 m from
  the nearest parcel, so it is a predicate/location residual rather than an H3 block
  failure. One seeded selection from the 148 PIP edges then traversed the real
  source-WKB-to-Canon materializer with independent digest verification and bounded
  fixed-point errors. Franklin-specific names remain outside generic materializer,
  composition, solver, and tile modules.

  This materially starts the typical-county tier but does not complete E5. Candidate
  reach is upstream of truth accuracy; the deed-grain truth plane, exact-local
  predicate replay, multi-source incidence, actual solver evaluation, and the
  coverage/precision curve remain unmeasured. Successful MCP calls also still lack
  query ids, preventing durable live receipt promotion. The parcel-free case remains
  relevant: missing parcels must suppress parcel answers, not discard useful
  building/address observations. FEMA is pinned to the
  Ohio-applicable 2023-05-02 partition rather than silently inheriting the unrelated
  global 2025-06-06 date.

Kill condition, stated honestly: if E3 — with the attribute channel joined and an
address-set layer landed — cannot rank the true lot above the wrong one on the majority of
E1's addressable failure classes, then the bits do not sum, the 500× premise of §13 fails,
and the architecture survives only as an honest-abstention engine, which is not the
product. That is the falsifiable form of "could this possibly work."

> **CURRENT STATUS — E1–E3 MEASURED; ORIGINAL RE-RANKING CLAIM REJECTED; E4/E5 OPEN.**
> Appendix L records 0/7 true-lot wins on tile-addressable E3 failures, so the narrow claim
> that joint landed evidence repairs point resolution by candidate re-ranking failed. The
> same taxonomy shows 72/79 labeled failures were unreachable or ledger-representation
> cases, so that experiment did not test the remaining collateral-composition claim. This
> is not a waiver of the kill condition: point re-ranking is no longer a product premise.
> E4 must now test composition on cases where multiple parcels/buildings are genuinely in
> scope, and E5 must establish the evidence-tier curve outside NYC. Failure there leaves an
> abstention/representation compiler, not the proposed constraint-resolution product.
>
> **2026-08-28 — E4 solver capability repaired and reverified
> (bd-2kjx.1–.3).** The composition
> kernel now decomposes the variable space over the constraint-incidence graph,
> solves components exactly inside declared budgets, falls back to a typed
> `BudgetFallback` for oversized coupled components, and reports whether each
> residual count and backbone is exact. Saturated counts are declared lower
> bounds. The earlier implementation incorrectly conflated `u64` saturation
> with infeasibility and used fixed-width selection masks; boundary and
> disconnected-product counterexamples now cover those defects. The current
> 17-case E4 harness solves without fallback and preserves every reachable
> truth under admitted hard evidence, but reaches only 9/17 truths. A proposed
> GeoDISC hard contract also has one empirical falsification and therefore
> remains diagnostic. The explicit 79-case acceptance test remains open, as
> do truth-instrument cleanup and the E5 evidence-tier curve.

---

## 18. Course correction (2026-09-01): the product is collateral composition, evidence-dated existence, and named conflicts

Added 2026-09-01 after the reality check of the implemented workbench against this plan.
This section is normative for scope. Where an earlier section proposes machinery this
section defers or cuts, this section controls. Nothing here weakens a gate, a denominator,
or a measurement; it changes what the gates are for.

### 18.1 What the measurements already decided

1. **Point re-ranking is not the product.** E1 found 72/79 labeled failures unreachable by
   any tile-local solver (40 gross-location inputs, 32 condo ledger residues). E3 ranked
   the true lot first 0/7 times on the reachable remainder. L.6 reached the high nineties
   on answered points with geometry, the entity-grain rule, and abstention, with no solver.
   M.5 showed PAD confirms the PIP lot 20/21 times on "gross" points, so much of that class
   is truth contamination. The constraint machinery has nothing to fix at point grain.
2. **The unsolved question is composition.** Which parcels and buildings constitute the
   collateral of one loan, at each entity level, with the ambiguity counted. Cases 3, 4,
   and 6 are the shape. The H.7 multi-BBL cohort is the population. As of this date the
   forward cohort has 71 genuine subjects, all 70 nonempty candidate universes solve
   exactly, and every one reports `evidence_no_observation`. The solver has never been
   given evidence on real multi-parcel collateral. That experiment is the whole question.
3. **The shipped solver is an extensional kernel.** `src/geo/composition.rs` factorizes
   the incidence graph, enumerates components exhaustively under a mask budget, falls back
   to pruned depth-first search, and reports exact counts and backbones. Its constraint
   vocabulary is Require, Forbid, Cardinality, AllowedSets, AnyOf, IntegerSumBand,
   AllOrNone, and Requires. None of §7's global propagators, none of §8's explanation
   artifacts, and none of §8.1's certificates exist in code, and until this section none
   had a bead.
4. **The cost thesis is not admissible.** §13's figures were falsified by Appendices C
   and G. Component maxima of 109 raw predicate-incidence nodes at r8 and 118 to 128 with
   Overture are observation stress bounds, not latent-variable widths. Nothing in the
   commercial argument may cite a per-tile time or a national cost until E4 records
   component costs with evidence admitted.
5. **Honest abstention is table stakes, not differentiation.** Commercial property APIs
   already return explicit nulls and separate "no coverage" from "no match." Abstention
   alone does not sell. Composition, counted ambiguity, named conflicts, building grain,
   dated physical existence, and deed-anchored truth are the capabilities no
   address-to-parcel API offers.

### 18.2 The product, restated

Three deliverables, in the order they can exist:

| # | Deliverable | What it is | First buyer |
|---|---|---|---|
| P1 | **Physical collateral ledger** | For every loan in every public CMBS deal: the exact parcel set, building set, deed-confirmed BBL or county parcel ids, the dated image vintage at which each structure was last observed present, the residual model count, the claim class, and the SEC accession the loan came from. Rows are receipts, not scores. | B-piece buyers and special servicers running workouts; issuer counsel attaching it to a Reg AB tape |
| P2 | **Explanation artifacts** | Backbone plus exact count; minimal blame set when admitted sources conflict (MUS); minimal repair set (MCS); counterfactual separation per prospective observation. | Data vendors and trustees; the same B-piece buyers for contested loans |
| P3 | **Evidence-tier curve per geography** | Coverage, precision, and abstention reported per evidence tier, with the cheapest decision-changing acquisition named per abstention. | Anyone pricing a non-NYC pool; the operator deciding which dataset to buy next |

CMBS capabilities that fall out of P1 and P2 and that no current vendor shows:

| Capability | Built from | Gate it waits on |
|---|---|---|
| Event exposure at loan grain within hours (storm wind radii against exact building sets, not address centroids) | P1 building sets plus the existing `geo_storm_exposure` advisory geometry | G3 |
| Dated collateral existence proof ("the structure described in the 2019 offering document is not present in the 2024 ortho") | P1 plus the imagery observer lane (§18.4) | G6 |
| Cross-deal collateral collision and adjacency concentration (one parcel in two trusts; one block across four deals) | P1 parcel sets across deals | G3 |
| Conflict proofs sent upstream to Overture, FEMA, assessors | P2 MUS | G5 |
| Ambiguity as a diligence signal (doubleton collateral means the offering document is ambiguous about what secures the debt) | P1 model count | G4 |

The moat is P2 plus the corpus P1 accumulates. A competitor with a probabilistic model can
approximate a backbone. Nobody can approximate a minimal repair set, and the blame sets
improve the public sources they are sent to.

### 18.3 Scope decisions

Everything in this plan is now in exactly one of three states. `IN` items have or must
receive a bead. `DEFERRED` items have a named trigger and a placeholder bead at P4 so they
cannot be silently lost. `CUT` items may be cited only as history.

**IN (v1 scope):**

- Truth plane rebuild with lender/party evidence and stratified adjudication (bd-179b).
- Evidence stacking onto the H.7 cohort without importing held-out ACRIS truth; E4 read on
  real evidence (bd-1g4x, bd-7bcp, bd-1l4r).
- Propagators over the existing extensional kernel, as domain filters that run before
  search and emit typed prunings: additive integer band feasibility (subset-sum bounds
  over `IntegerSumBand`), cardinality bounds (`gcc`-style lower/upper counts per level and
  per source), within-source exclusivity with a Hall-set check on small components. Search
  remains the exact backend. (new bead)
- Explanation artifacts: deletion-based minimal unsatisfiable core with the declared
  source-reliability order (QuickXplain semantics), minimal correction sets by hitting
  sets over enumerated cores, both bounded by deterministic counters with a typed
  "oversized core" fallback. (new bead)
- Imagery and elevation observer lane per §18.4. (new beads)
- Condo ledger bridge: unit BBL to billing BBL to BIN with block/geometry confirmation
  (bd-2fed).
- Abstain, re-geocode, retry loop as a bounded acquisition step (new bead).
- `canon geo inspect` and residual-aware next evidence (bd-1g18, bd-vojr).
- E5 in Franklin County with deed-grain truth and the tier curve (bd-s07o).
- Physical collateral ledger output surface and the cross-deal and event-exposure joins
  (new beads; bd-kwmc owns the client-facing shape).
- Incremental accretion and the parallel work protocol (bd-2rf9, bd-3oj1).

**DEFERRED (trigger named):**

- Compiled residual representation (reduced OBDD, SDD, d-DNNF). Trigger: a measured
  component on real evidence reaches `BudgetFallback` under pruned search, or an
  explanation query needs polytime entailment. bd-19wp's benchmark stands: no order or
  vtree is frozen.
- Latent-slot symmetry breaking (§5.1). Trigger: latent buildings become solver
  variables. Today observations are the variables and the canonical order already breaks
  observation symmetry.
- Allen/STP temporal solver (§7.1). Trigger: the observer lane supplies dated existence
  observations for a population. Interval admission stays implemented and diagnostic.
- VeriPB proof logs (§8.1). Trigger: a trustee, rating agency, or regulator asks for a
  third-party-checkable certificate in writing.
- National cost model (§13). Trigger: E4 and E5 component costs recorded with evidence.
- POI and tenant evidence (§16.3 row 10). Trigger: a source lands.

**CUT:**

- Point re-ranking as a product premise (Appendix L).
- The 0.5 s per tile, 140 CPU-hour, and "few hundred dollars" claims.
- Rendered basemap screenshots (Google, Mapbox, Esri, Apple) as evidence of any kind;
  Appendix J quotes the governing clauses.
- Any vision model output used as a proposer of location.

### 18.4 The imagery and map evidence lane

Appendix A stated the rule: a model is a source, not a solver. Appendix J verified the
sources. This section makes the lane buildable.

**Sources, in order of first use.** NYS and NYC 6-inch orthoimagery (CC BY, even years
2006 to 2024, byte-range capable with ETags) for the proving ground. 3DEP lidar for measured
height. NAIP through the Planetary Computer mirror as the national fallback. NOAA
emergency response imagery for event-scoped change. Overture or OSM data for street
context, never a rendered commercial tile. Every image tile is pinned by URL, byte range,
ETag or SHA-256, vintage, and license text hash.

**Observer contract.** An observer is a deterministic function from pinned image bytes
and a pinned geometry window to typed observations. It declares: model identity and
weight hash (or "rule-based"), input tile digests, output observation kinds, the
population on which its regional error was characterized, the characterization digest, and
the `rho` band each observation kind induces. A vision model with frozen hashed weights in
controlled arithmetic qualifies. A hosted model whose output cannot be reproduced
byte-for-byte also qualifies, but only as a **recorded observation with provenance**:
Canon stores the label, the crop digest, the model version, and the prompt digest, and
replay never re-runs the model. In both cases the observation enters the solver only
through `rho`.

**Observation kinds** (each with its sound reading):

| Kind | Sound `rho` reading | Feeds |
|---|---|---|
| `structure_count_in_window` with error band | `gcc` lower/upper bound on latent structures inside the parcel or window | cardinality propagator |
| `footprint_outline` at vintage | one more footprint plane with its own within-source exclusivity; majority-parcel predicate applies | candidate universe, incidence |
| `height_or_floors` from 3DEP with density-derived error | integer band on floors; never a decider | additive band |
| `present_at_vintage` / `absent_at_vintage` | closed valid-time interval observation; diagnostic until the temporal solver lands, hard only as "absent at v implies not the 2019 structure" once dated | interval admission; later Allen/STP |
| `change_event` between two vintages | diagnostic flag that raises the next-evidence priority; never a constraint alone | next-evidence controller |

**Three uses, in order.** First, adjudication: ortho crops with candidate parcel lines
drawn on them are the cheapest stratified truth instrument for bd-179b's second source;
the crop and the label are the receipt. Second, the visual evidence card (bd-101v): ortho,
candidate parcels, forced set, ambiguous set, conflicting source records, all overlaid,
which is both the reviewer surface and the sales artifact. Third, solver input through the
kinds above, only after the observer's error is characterized on a named population.

**What stays forbidden.** A screenshot of a commercial basemap; a model answering "the
property is here"; any observer whose output is re-generated at replay; any imagery
observation admitted without a characterized error population.

### 18.5 The solver middle layer

The extensional kernel is kept as the exact backend. Three additions make it the product:

1. **Propagators as pre-search filters.** Each runs to a fixpoint over the component's
   domains before enumeration, prunes values with a typed reason naming the constraint and
   the evidence ids, and never changes the model set (soundness is checked in tests by
   comparing enumeration with and without propagation on every fixture). They shrink the
   search space and produce the explanation skeleton for free.
2. **Minimal cores and repairs.** On an empty model set, compute one minimal
   unsatisfiable subset of admitted constraints by deletion under the declared reliability
   order, then enumerate correction sets as minimal hitting sets while the core count stays
   under a deterministic ceiling. The artifact names source records, not constraint
   indices. Oversized cases return a typed superset with `explanation_complete=false`.
3. **Counterfactual separation.** For a declared prospective observation with an
   exhaustive outcome domain, report the exact model count under each outcome. Never call
   it expected value without calibrated probabilities (architecture §9).

### 18.6 What this changes in the build order

Truth plane first, evidence onto the cohort second, propagators and explanations third,
observer lane fourth, ledger and CMBS joins fifth, E5 throughout. §19 states the sequence
with gates.

---

## 19. Execution plan: stages, gates, invariants, and bead ownership

Added 2026-09-01. This section exists so that an agent can pick up any stage without
re-deriving design decisions. Beads carry the full text of their stage; this section is
the index and the frozen gate definitions.

### 19.1 Non-negotiables

- N01 Runtime lookup stays exact registry replay. Geo is a build-time workbench.
- N02 Same input plus same pinned sources reproduce byte-identical artifacts on every
  platform. No wall clock, locale, hash-map order, or float comparison in a decision path.
- N03 Every source value enters the solver only through a declared, versioned `rho`.
- N04 Adding admitted hard evidence may only narrow the model set or make it empty.
- N05 Source count is provenance, never evidence weight or confidence.
- N06 Candidate reach, admission, solver exactness, reconciliation, truth quality, and
  cost are reported as separate planes and never pooled.
- N07 Fixtures, retained evidence, and mocked providers are never presented as live proof.
- N08 Abstention is a legitimate output; an abstention-only module is not a delivered
  feature.
- N09 No vision or statistical model proposes a location. Models are observers with
  characterized error.
- N10 No commercial basemap imagery or tiles enter any artifact.
- N11 Gates are frozen in tests. A gate is passed by meeting it, never by editing it.
- N12 Every feature bead ships its code and its tests together, including at least one
  negative case a naive implementation fails.

### 19.2 Invariants for the new artifacts

- I01 A propagator prunes a value only with a typed reason naming constraint id and
  evidence ids; pruning is sound: enumeration with propagation equals enumeration without.
- I02 Propagator fixpoint is order-independent; tests run every permutation of propagator
  order on each fixture and compare bytes.
- I03 A minimal core is minimal: removing any one member restores satisfiability; tests
  verify by re-solving each deletion.
- I04 A correction set hits every enumerated core; when core enumeration hits its ceiling
  the artifact says `cores_complete=false` and no minimality claim is made.
- I05 Explanation artifacts name source record ids and rho contract ids, never internal
  indices.
- I06 An observer observation carries image tile digest, vintage, license hash, model or
  rule identity, weight or prompt digest, and the error-population digest; missing any
  field refuses.
- I07 Observer output is stored, never regenerated at replay; replay verifies the stored
  digest chain only.
- I08 A ledger row binds accession, loan id, parcel set, building set, truth plane,
  claim class, model count, exactness flags, and every source release pin; rows with
  candidate reach `none` carry the reason and no sets.
- I09 A ledger row never pools truth planes; non-round and round exact-lender evidence
  stay labeled.
- I10 The retry loop is bounded by a declared pass count; each pass is a normal pinned
  run and the loop artifact lists every pass with its abstention reason.
- I11 Event exposure joins building geometry, not centroids; advisory source hashes travel
  with the result.
- I12 Cross-deal collision reports a parcel or building shared by more than one trust
  with both accessions; pari passu participation is a labeled explanation, not a suppressed
  row.
- I13 `geo inspect` reads only emitted artifacts and receipts; it computes nothing that
  changes an answer.
- I14 Next-evidence recommendations expose the nondominated frontier and never manufacture
  a total ranking without a declared loss model.

### 19.3 Module skeleton

**Dependency direction.** New modules depend on the existing `composition`, `evidence`,
`control`, `discovery`, `evaluation`, `geometry`, `geometry_value`, and `run` modules,
never the reverse. `src/geo/composition.rs` and `src/geo/evidence.rs` are not modified by
D2 to D6: `propagate` and `explain` call `solve_composition` as a black box and narrow or
re-solve `GeoCompositionRequest` values. Within the new set: `explain` consumes
`propagate`; `card`, `next_evidence`, and `inspect` consume `explain`; `exposure` and
`collision` consume `ledger`; `adjudicate` and `card` consume `observer`; nothing consumes
`inspect`.

**Shared shape.** Each module is `pub mod` in `src/geo/mod.rs` with `pub use module::*`.
Each defines `Geo<Module>Error { code: Geo<Module>ErrorCode, message: String, detail:
BTreeMap<String, String> }` with a `#[serde(rename_all = "snake_case")]` code enum that
carries the generic `UnsupportedVersion`, `InvalidInput`, `BudgetExceeded`,
`ArithmeticOverflow` variants plus the module's rows from §19.4. Each artifact type has a
`version: String` constant `CANON_GEO_<NAME>_VERSION`, `canonical_<name>_bytes`, and
`validate_<name>_artifact`. Each module ships `schemas/canon.geo.<name>.v0.schema.json`, a
`--describe` entry, a `canon geo <subcommand>`, and `tests/geo_<module>.rs`.

| Module | Stage | Responsibility | Contract ids |
|---|---|---|---|
| `src/geo/propagate.rs` | D2 | additive-band, cardinality, exclusivity propagators; fixpoint driver; typed prunings | `canon_geo_propagation.v0` |
| `src/geo/explain.rs` | D2 | minimal core, correction sets, counterfactual separation | `canon_geo_explanation.v0`, `canon_geo_separation_request.v0`, `canon_geo_separation.v0` |
| `src/geo/observer.rs` | D6 | observer contract, observation admission, image tile pinning, license gate | `canon_geo_observer.v0`, `canon_geo_observation_rows.v0`, `canon_geo_image_tile_pin.v0` |
| `src/geo/adjudicate.rs` | D0, D6 | adjudication crop requests and label receipts for the truth plane | `canon_geo_adjudication_request.v0`, `canon_geo_adjudication_receipt.v0` |
| `src/geo/card.rs` | D6 | visual evidence card artifact (data, not rendering) | `canon_geo_evidence_card.v0` |
| `src/geo/ledger.rs` | D3 | physical collateral ledger rows and deal-level rollups | `canon_geo_collateral_ledger.v0` |
| `src/geo/exposure.rs` | D3 | event exposure join over ledger building sets | `canon_geo_event_exposure.v0` |
| `src/geo/collision.rs` | D3 | cross-deal parcel and building collision, adjacency concentration | `canon_geo_cross_deal.v0` |
| `src/geo/retry.rs` | D4 | abstain, re-geocode request, retry loop artifact | `canon_geo_retry_loop.v0` |
| `src/geo/inspect.rs` | D4 | one-call run state, compare | `canon_geo_inspection.v0` |
| `src/geo/next_evidence.rs` | D5 | nondominated next-action frontier, stop decisions | `canon_geo_next_evidence.v0` |
| `src/geo/condo.rs` | D4 | unit to billing lot to building crosswalk with confirmation | `canon_geo_ledger_bridge.v0` |

Per-module surface. Field lists are the required minimum; implementers may add
`#[serde(default)]` fields, never remove or rename these.

**`propagate.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCompositionRequest`, `GeoCompositionUniverse`, `GeoHardConstraint`, `GeoHardConstraintKind::{IntegerSumBand, Cardinality, AllowedSets, Require, Forbid, AllOrNone, Requires}`, `GeoEntityRef`, `GeoEntityLevel`; optional `GeoEvidenceCompilationArtifact` whose `admissions[].contract.source_dataset` and `generated_ids` group constraints by source for exclusivity |
| `GeoPropagatorKind` | `AdditiveBand`, `Cardinality`, `SourceExclusivity` |
| `GeoPropagationBudget` | `max_fixpoint_rounds: u64`, `max_hall_subset_size: usize`, `max_subset_sum_states: u64` |
| `GeoPrunedValue` | `Excluded`, `Forced` |
| `GeoPruning` | `member: GeoEntityRef`, `value: GeoPrunedValue`, `propagator: GeoPropagatorKind`, `constraint_ids: Vec<String>`, `evidence_ids: Vec<String>` (observation ids when evidence is supplied, else empty) |
| `GeoPropagationFallback` | `propagator: GeoPropagatorKind`, `counter: String`, `configured: u64`, `guidance: String` |
| `GeoPropagationArtifact` | `version`, `request_blake3`, `prunings: Vec<GeoPruning>` (sorted by member then value), `rounds: u64`, `fixpoint_reached: bool`, `budget_fallback: Option<GeoPropagationFallback>`, `counters: BTreeMap<String, u64>` |
| `pub fn propagate(request: &GeoCompositionRequest, evidence: Option<&GeoEvidenceCompilationArtifact>, budget: &GeoPropagationBudget) -> Result<GeoPropagationArtifact, GeoPropagationError>` | runs all three propagators to a fixpoint per incidence component; every pruning cites at least one constraint id |
| `pub fn apply_prunings(request: &GeoCompositionRequest, artifact: &GeoPropagationArtifact) -> Result<GeoCompositionRequest, GeoPropagationError>` | returns a narrowed request: `Excluded` becomes `GeoHardConstraintKind::Forbid`, `Forced` becomes `Require`, ids prefixed `prune:`; universe and `max_assignments` unchanged so `model_satisfies_request` stays comparable |
| `pub fn check_soundness(request: &GeoCompositionRequest, artifact: &GeoPropagationArtifact) -> Result<GeoSoundnessReport, GeoPropagationError>` | solves original and narrowed requests with `solve_composition`, compares `residual_models` and `summary.residual_model_count`; `GeoSoundnessReport { sound: bool, model_count_before: u64, model_count_after: u64, differing_models: Vec<GeoCompositionModel> }` (I01, T03) |

**`explain.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCompositionRequest`, `GeoCompositionArtifact` (`status == GeoCompositionStatus::Conflict`, `conflict_constraint_ids`, `conflict_core_complete`), `GeoEvidenceCompilationArtifact` (`GeoEvidenceAdmission { observation_id, contract, source_records, generated_ids }` maps constraint ids to `GeoEvidenceRecordRef.source_record_id` and `GeoRhoContract.id`), `GeoPropagationArtifact` (pruning skeleton), `solve_composition` |
| `GeoReliabilityOrder` | `contract_ids_most_reliable_first: Vec<String>`; every admitted contract id must appear exactly once |
| `GeoExplanationBudget` | `max_core_solves: u64`, `max_cores: u64`, `max_hitting_sets: u64` |
| `GeoMinimalCore` | `constraint_ids`, `observation_ids`, `source_record_ids`, `rho_contract_ids`, `minimal: bool` |
| `GeoCorrectionSet` | `observation_ids`, `source_record_ids`, `minimal: bool` |
| `GeoExplanationArtifact` | `version`, `request_blake3`, `evidence_blake3`, `cores: Vec<GeoMinimalCore>`, `cores_complete: bool`, `correction_sets: Vec<GeoCorrectionSet>`, `explanation_complete: bool`, `counters: BTreeMap<String, u64>` |
| `GeoProspectiveOutcome` | `outcome_id: String`, `induced: Vec<GeoHardConstraintKind>` |
| `GeoProspectiveObservation` | `id`, `contract_id`, `cost_units: u64`, `outcomes: Vec<GeoProspectiveOutcome>` (declared exhaustive) |
| `GeoSeparationRequest` | `version`, `request: GeoCompositionRequest`, `prospective: Vec<GeoProspectiveObservation>` |
| `GeoOutcomeSeparation` | `outcome_id`, `residual_model_count: u64`, `count_exact: bool` |
| `GeoSeparationArtifact` | `version`, `request_blake3`, `baseline_model_count`, `per_observation: Vec<{ observation_id, per_outcome: Vec<GeoOutcomeSeparation>, worst_case_remaining: u64, redundant: bool }>` |
| `pub fn minimal_core(request: &GeoCompositionRequest, evidence: &GeoEvidenceCompilationArtifact, order: &GeoReliabilityOrder, budget: &GeoExplanationBudget) -> Result<GeoExplanationArtifact, GeoExplanationError>` | deletion-based core under `order` (QuickXplain semantics); refuses unless the input solves to `Conflict` (I03, I05) |
| `pub fn correction_sets(artifact: &mut GeoExplanationArtifact, request: &GeoCompositionRequest, evidence: &GeoEvidenceCompilationArtifact, budget: &GeoExplanationBudget) -> Result<(), GeoExplanationError>` | enumerates further cores up to `max_cores`, then minimal hitting sets; sets `cores_complete` and `explanation_complete` (I04) |
| `pub fn separate(request: &GeoSeparationRequest, budget: &GeoExplanationBudget) -> Result<GeoSeparationArtifact, GeoExplanationError>` | one `solve_composition` per outcome with `induced` appended; exact only when the baseline residual is complete and unsaturated; never emits an expected value (§18.5 item 3) |

**`observer.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoRhoContract`, `GeoRhoObservation`, `GeoRhoObservationKind::IntegerSumBand`, `GeoValidTimeInterval`, `GeoEvidenceRecordRef`, `GeoIntegerMeasure`, `GeoIntegerValueOrigin::SourceAsserted`, `GeoCanonicalPolygonMm` for the window |
| `GeoImageTilePin` | `url`, `byte_range: Option<(u64, u64)>`, `etag: Option<String>`, `blake3`, `vintage: GeoValidTimeInterval`, `license_id`, `license_text_blake3` |
| `GeoObserverIdentity` | `RuleBased { rule_id, rule_version }`, `FrozenWeight { model_id, weight_blake3, arithmetic_contract }`, `RecordedHosted { model_id, model_version, prompt_blake3 }` |
| `GeoObservationKind` | `StructureCountInWindow`, `FootprintOutline`, `HeightOrFloors`, `PresentAtVintage`, `AbsentAtVintage`, `ChangeEvent` (§18.4 table) |
| `GeoObserverContract` | `id`, `version`, `identity: GeoObserverIdentity`, `output_kinds: Vec<GeoObservationKind>`, `error_population_id`, `characterization_blake3`, `rho_contract_ids: Vec<String>` |
| `GeoObservationRow` | `id`, `observer_id`, `tile_pins: Vec<GeoImageTilePin>`, `window_blake3`, `kind`, `payload: GeoObservationPayload` (one variant per kind: count band `{min, max}`, ring digest, floors band, vintage interval, vintage pair), `crop_blake3`, `label_blake3` |
| `GeoObservationRowsArtifact` | `version`, `contract: GeoObserverContract`, `rows: Vec<GeoObservationRow>`, `rho_observations: Vec<GeoRhoObservation>`, `diagnostic_only_ids: Vec<String>`, `not_admitted_ids: Vec<String>` |
| `pub fn admit_observations(contract: &GeoObserverContract, rows: &[GeoObservationRow], rho: &[GeoRhoContract], forbidden_license_ids: &[String]) -> Result<GeoObservationRowsArtifact, GeoObserverError>` | applies I06; every row's `kind` must be in `output_kinds`; every `rho_contract_ids` entry must exist in `rho` |
| `pub fn to_rho_observation(row: &GeoObservationRow, contract: &GeoObserverContract, universe: &GeoCompositionUniverse) -> Option<GeoRhoObservation>` | `StructureCountInWindow` and `HeightOrFloors` become `IntegerSumBand` (`unit` `structure` with value 1 per building candidate in the window, or `floor`; `value_origin` `SourceAsserted`); `PresentAtVintage`/`AbsentAtVintage` carry `valid_time` and are listed in `diagnostic_only_ids`; `FootprintOutline` and `ChangeEvent` return `None` (they enter through `materialize` and `next_evidence`) |
| `pub fn verify_replay(artifact: &GeoObservationRowsArtifact, bytes_by_blake3: &BTreeMap<String, Vec<u8>>) -> Result<(), GeoObserverError>` | recomputes tile, crop, and label digests; contains no call path to any identity (I07) |

**`adjudicate.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoImageTilePin`, `GeoTruthPlane::HumanAdjudication`, `GeoCanonicalPolygonMm` for candidate parcel lines, population case ids from `canon_geo_population.v0` |
| `GeoAdjudicationRequest` | `version`, `case_id`, `subject_id`, `tile_pin: GeoImageTilePin`, `window_blake3`, `candidate_parcel_ids: Vec<String>`, `overlay_geometry_blake3` |
| `GeoAdjudicationLabel` | `SelectedParcels(Vec<String>)`, `NoneVisible`, `Unresolvable` |
| `GeoAdjudicationReceipt` | `version`, `request_blake3`, `crop_blake3`, `label`, `adjudicator_id`, `truth_plane: GeoTruthPlane`, `notes_blake3: Option<String>` |
| `pub fn build_adjudication_requests(cases: &[(String, String, Vec<String>)], pins: &BTreeMap<String, GeoImageTilePin>, overlays: &BTreeMap<String, GeoCanonicalPolygonMm>) -> Result<Vec<GeoAdjudicationRequest>, GeoAdjudicationError>` | tuple is `(case_id, subject_id, candidate_parcel_ids)`; deterministic order by `case_id` |
| `pub fn validate_adjudication_receipt(request: &GeoAdjudicationRequest, receipt: &GeoAdjudicationReceipt, crop_bytes: &[u8]) -> Result<(), GeoAdjudicationError>` | request digest, crop digest, `truth_plane == HumanAdjudication`, and label parcels within `candidate_parcel_ids` |

**`card.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCompositionArtifact` (`hard_forced`, `residual_models`, `conflict_constraint_ids`), `GeoEvidenceCompilationArtifact`, `GeoExplanationArtifact`, `GeoImageTilePin`, `GeoTypedGeometry` |
| `GeoEvidenceCard` | `version`, `subject_id`, `ortho_pin: GeoImageTilePin`, `candidate_parcels: Vec<{ id, geometry_blake3 }>`, `forced: GeoCompositionBackbone`, `ambiguous_members: Vec<GeoEntityRef>`, `conflicting_records: Vec<GeoEvidenceRecordRef>`, `composition_blake3`, `evidence_blake3`, `explanation_blake3: Option<String>` |
| `pub fn build_evidence_card(subject_id: &str, composition: &GeoCompositionArtifact, evidence: &GeoEvidenceCompilationArtifact, explanation: Option<&GeoExplanationArtifact>, ortho: &GeoImageTilePin, geometry: &BTreeMap<String, GeoTypedGeometry>) -> Result<GeoEvidenceCard, GeoCardError>` | data only; `ambiguous_members` is the universe minus backbone minus members absent from every residual model |

**`ledger.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCompositionArtifact` (`status`, `summary.residual_model_count`, `residual_model_count_complete`, `residual_model_count_saturated`, `hard_forced`, `backbone_complete`), `GeoEntityProjection`, `GeoEvidenceCompilationArtifact`, `GeoCandidateReachStatus`, `GeoTruthPlane`, `GeoClaimClass`, `GeoValidTimeInterval`, release pins from `GeoRegionalInventory` |
| `GeoSourceReleasePin` | `source_dataset`, `source_release`, `blake3` |
| `GeoLedgerRow` | `version`, `accession`, `deal_id`, `loan_id`, `reach: GeoCandidateReachStatus`, `reach_none_reason: Option<String>`, `parcel_set: Option<Vec<String>>`, `building_set: Option<Vec<String>>`, `deed_ids: Vec<String>`, `truth_plane: Option<GeoTruthPlane>`, `claim_class: GeoClaimClass`, `residual_model_count: u64`, `count_exact: bool`, `backbone_complete: bool`, `last_observed_present: Option<GeoValidTimeInterval>`, `source_release_pins: Vec<GeoSourceReleasePin>`, `composition_blake3`, `evidence_blake3` |
| `GeoDealRollup` | `deal_id`, `accession`, `rows: u64`, per-`GeoTruthPlane` counts of `resolved`, `ambiguous`, `conflict`, `reach_none` (never a pooled total) |
| `GeoCollateralLedger` | `version`, `rows: Vec<GeoLedgerRow>` (sorted by accession, loan id), `rollups: Vec<GeoDealRollup>` |
| `pub fn build_ledger_row(loan: &GeoLedgerLoanRef, reach: GeoCandidateReachStatus, reach_none_reason: Option<String>, composition: Option<&GeoCompositionArtifact>, evidence: Option<&GeoEvidenceCompilationArtifact>, truth_plane: Option<GeoTruthPlane>, pins: &[GeoSourceReleasePin]) -> Result<GeoLedgerRow, GeoLedgerError>` | `GeoLedgerLoanRef { accession, deal_id, loan_id, deed_ids }`; `reach == None` requires a reason and both sets `None`; otherwise both artifacts required (I08, I09) |
| `pub fn roll_up_deal(rows: &[GeoLedgerRow]) -> Result<GeoDealRollup, GeoLedgerError>` | one deal per call; refuses mixed `deal_id` |
| `pub fn validate_ledger(ledger: &GeoCollateralLedger) -> Result<(), GeoLedgerError>` | I08, I09, sort order, rollup consistency |

**`exposure.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCollateralLedger` building sets, `GeoCanonicalPolygonMm` and `GeoLinearRingMm` building geometry from `GeoGeometryTileArtifact`, geometry.rs predicates |
| `GeoWindRadiusRing` | `knots: u16`, `ring: GeoCanonicalPolygonMm` |
| `GeoAdvisoryPin` | `advisory_id`, `storm_id`, `advisory_number: u32`, `issued: GeoValidTimeInterval`, `source_blake3s: Vec<String>`, `wind_radii: Vec<GeoWindRadiusRing>` |
| `GeoExposedBuilding` | `accession`, `loan_id`, `building_id`, `knots_band: u16` (highest ring containing the building) |
| `GeoEventExposure` | `version`, `advisory: GeoAdvisoryPin`, `ledger_blake3`, `exposed: Vec<GeoExposedBuilding>`, `buildings_without_geometry: Vec<String>` |
| `pub fn join_exposure(ledger: &GeoCollateralLedger, advisory: &GeoAdvisoryPin, geometry: &BTreeMap<String, GeoCanonicalPolygonMm>, archive_blake3s: &[String]) -> Result<GeoEventExposure, GeoExposureError>` | polygon-in-ring by exact geometry, never centroids (I11); `archive_blake3s` must contain every `source_blake3s` entry |

**`collision.rs`**

| Item | Definition |
|---|---|
| Consumes | two or more `GeoCollateralLedger` values, `GeoEntityRef` |
| `GeoCollisionKind` | `SharedParcel`, `SharedBuilding`, `Adjacent` |
| `GeoPariPassuDeclaration` | `entity: GeoEntityRef`, `accessions: Vec<String>`, `source_record: GeoEvidenceRecordRef` |
| `GeoCollision` | `kind`, `entity: GeoEntityRef`, `accessions: Vec<String>`, `loan_ids: Vec<String>`, `pari_passu: bool`, `explanation: String` |
| `GeoCrossDealArtifact` | `version`, `ledger_blake3s: Vec<String>`, `collisions: Vec<GeoCollision>`, `adjacency_concentration: Vec<{ block_id, deal_count: u64, accessions }>` |
| `pub fn find_collisions(ledgers: &[GeoCollateralLedger], declarations: &[GeoPariPassuDeclaration], adjacency: &BTreeMap<String, String>) -> Result<GeoCrossDealArtifact, GeoCollisionError>` | `adjacency` maps parcel id to block id; a declared pari passu match sets `pari_passu = true` and keeps the row (I12) |

**`retry.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoRun` (`status`, `blockers`, `next_actions`, `semantic_hash`), `GeoAcquisitionRequest`, `GeoAcquisitionReceipt` |
| `GeoRetryPolicy` | `max_passes: u8`, `regeocode_request_template: GeoAcquisitionRequest` |
| `GeoRetryPass` | `index: u8`, `plan_blake3`, `run_blake3`, `abstention_reason: String`, `regeocode: Option<GeoAcquisitionRequest>`, `receipt_blake3: Option<String>` |
| `GeoRetryTerminal` | `Resolved`, `AbstainedAtCeiling`, `Blocked` |
| `GeoRetryLoopArtifact` | `version`, `subject_id`, `policy: GeoRetryPolicy`, `passes: Vec<GeoRetryPass>`, `terminal: Option<GeoRetryTerminal>` |
| `pub fn next_retry_pass(loop_state: &GeoRetryLoopArtifact, latest_run: &GeoRun) -> Result<Option<GeoAcquisitionRequest>, GeoRetryError>` | `Some` only while `passes.len() < max_passes` and the run abstained; the loop emits requests and never geocodes (I10) |
| `pub fn record_pass(loop_state: &mut GeoRetryLoopArtifact, run: &GeoRun, receipt: Option<&GeoAcquisitionReceipt>) -> Result<(), GeoRetryError>` | appends one pass; sets `terminal` |

**`inspect.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoRun`, `canon.project.run.v2` receipts, `GeoCompositionArtifact`, `GeoEvidenceCompilationArtifact`, `GeoExplanationArtifact` (all read from the work directory by digest) |
| `GeoInspectionQuestion` | `Q1` to `Q8`, the eight questions in architecture §1, in that order |
| `GeoInspectionAnswer` | `question: GeoInspectionQuestion`, `answer: String`, `artifact_refs: Vec<GeoRunArtifactRef>` (at least one) |
| `GeoInspectionDelta` | `evidence_added`, `evidence_removed`, `components_invalidated`, `model_count_before: u64`, `model_count_after: u64`, `backbone_gained`, `backbone_lost`, `contradictions_introduced`, `contradictions_resolved`, `claim_class_changes: Vec<(GeoClaimClass, GeoClaimClass)>` |
| `GeoInspection` | `version`, `run_id`, `semantic_hash`, `answers: Vec<GeoInspectionAnswer>` (exactly eight), `compare: Option<GeoInspectionDelta>` |
| `pub fn inspect(work_dir: &Path) -> Result<GeoInspection, GeoInspectError>` | reads only; no `solve_composition` call path (I13) |
| `pub fn compare(base: &GeoInspection, other: &GeoInspection) -> Result<GeoInspectionDelta, GeoInspectError>` | both must share `plan_ref` question hash |

**`next_evidence.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoCompositionArtifact`, `GeoSeparationArtifact`, `GeoDecisionPolicyRef`, `GeoResourceBudget`, `GeoAbstentionPolicy`, `GeoAcquisitionRequest` |
| `GeoNextActionKind` | `Acquire(GeoAcquisitionRequest)`, `Adjudicate(String)`, `Observe(String)`, `Stop` |
| `GeoNextAction` | `action_id`, `kind`, `cost_units: u64`, `separation: Vec<GeoOutcomeSeparation>`, `dominated_by: Vec<String>` |
| `GeoStopReason` | `ClaimForced`, `AllActionsRedundant`, `GrainUnsupported`, `HonestAmbiguity`, `BudgetExceeded` (architecture §9) |
| `GeoNextEvidenceArtifact` | `version`, `run_id`, `frontier: Vec<GeoNextAction>`, `dominated: Vec<GeoNextAction>`, `total_ranking: Option<Vec<String>>` (`Some` only under a declared loss model), `stop: Option<GeoStopReason>` |
| `pub fn recommend(composition: &GeoCompositionArtifact, separation: &GeoSeparationArtifact, candidates: &[GeoNextAction], policy: Option<&GeoDecisionPolicyRef>, budget: &GeoResourceBudget) -> Result<GeoNextEvidenceArtifact, GeoNextEvidenceError>` | dominance by cost and per-outcome separation; frontier is always emitted (I14) |

**`condo.rs`**

| Item | Definition |
|---|---|
| Consumes | `GeoWarehouseGeometryRow`, `GeoLinearRingMm`, `footprint_majority_area_inside_parcel`, `GeoEntityRef`, `GeoIdentityRelation::{PartOf, On}`, `validate_identity_relation` |
| `GeoCondoBridgeRequest` | `version`, `unit_bbl`, `billing_bbl_candidates: Vec<String>`, `bin_candidates: Vec<String>`, `block: String`, `parcel_rings: BTreeMap<String, GeoLinearRingMm>`, `footprint_rings: BTreeMap<String, GeoLinearRingMm>` |
| `GeoCondoConfirmation` | `BlockAndGeometry`, `BlockOnly`, `KeyOnly` |
| `GeoLedgerBridge` | `version`, `unit_bbl`, `billing_bbl: Option<String>`, `bins: Vec<String>`, `confirmation: GeoCondoConfirmation`, `relations: Vec<(GeoEntityRef, GeoIdentityRelation, GeoEntityRef)>`, `abstained_reason: Option<String>` |
| `pub fn bridge_condo_unit(request: &GeoCondoBridgeRequest) -> Result<GeoLedgerBridge, GeoCondoError>` | emits `billing_bbl` and `bins` only under `BlockAndGeometry` (block match plus majority-area footprint containment); `BlockOnly` and `KeyOnly` abstain with sets empty (T11) |

### 19.4 Error taxonomy additions

Reason codes follow the existing `GeoEvidenceErrorCode` style: a `#[serde(rename_all =
"snake_case")]` enum per module, carried in `Geo<Module>Error { code, message, detail }`.
Every module reuses the generic `unsupported_version`, `invalid_input`, `budget_exceeded`,
and `arithmetic_overflow` variants for malformed input and counter overflow; the rows below
are the module-specific additions. Each row has exactly one class:

| Class | Meaning | Surface |
|---|---|---|
| refusal | no artifact is emitted; `Err(Geo<Module>Error)` with the code and a recovery hint in `detail` | nonzero exit, `emit_serialization_refusal` shape in `cli.rs` |
| abstention | an artifact is emitted with no answer for the affected unit and the code as its typed reason | zero exit; the code appears in a `*_reason` field |
| typed fallback | an artifact is emitted with a partial result and a completeness flag set to `false`; the code names the exhausted counter | zero exit; the code appears in a `*_fallback` field |

`detail` always carries the ids the code names (constraint, observation, source record,
tile, artifact, or pass) so the message is never the only carrier.

| Code | Module and function | Condition | Class | Test |
|---|---|---|---|---|
| `propagation_unsound_detected` | `propagate::check_soundness` | residual model set or count differs between the original and narrowed request | refusal; also fails T03 in CI | T03 |
| `propagation_budget_exhausted` | `propagate::propagate` | `max_fixpoint_rounds`, `max_hall_subset_size`, or `max_subset_sum_states` reached before fixpoint | typed fallback: `fixpoint_reached = false`, prunings so far retained (each is individually justified), `budget_fallback` set | T19 |
| `core_not_minimal` | `explain::minimal_core` | re-solving with any single core member deleted stays `Conflict` (I03 check fails) | refusal | T05 |
| `core_enumeration_ceiling` | `explain::correction_sets` | `max_cores` or `max_core_solves` reached before enumeration closes | typed fallback: `cores_complete = false`, `explanation_complete = false`, every `minimal` flag `false` (I04) | T06 |
| `explanation_not_conflict` | `explain::minimal_core` | input request solves to `Resolved` or `Ambiguous`; no core exists | refusal | T20 |
| `separation_residual_inexact` | `explain::separate` | baseline `residual_model_count_complete = false` or `residual_model_count_saturated = true`, or an outcome solve returns `BudgetFallback` | typed fallback: `count_exact = false` on the affected `GeoOutcomeSeparation`; no `redundant` claim | T21 |
| `observer_missing_provenance` | `observer::admit_observations` | any I06 field absent on the contract, a row, or a tile pin | refusal | T13 |
| `observer_error_uncharacterized` | `observer::admit_observations` | `error_population_id` or `characterization_blake3` empty, or a row `kind` not in `output_kinds` | refusal | T13 |
| `observer_license_forbidden` | `observer::admit_observations` | a tile pin `license_id` is in `forbidden_license_ids` or `license_text_blake3` is empty (N10) | refusal | T13 |
| `image_tile_digest_mismatch` | `observer::verify_replay`, `adjudicate::validate_adjudication_receipt` | bytes supplied for a pin, crop, or label hash differently from the stored `blake3` | refusal | T14 |
| `observation_regenerated_at_replay` | `observer::verify_replay` | caller supplies a row whose `label_blake3` or payload digest differs from the stored artifact, or requests identity invocation during replay (I07) | refusal | T14 |
| `observation_temporal_diagnostic` | `observer::to_rho_observation` | `PresentAtVintage` or `AbsentAtVintage` before dated composition exists | abstention: id listed in `diagnostic_only_ids`, no hard constraint (T15) | T15 |
| `adjudication_label_outside_candidates` | `adjudicate::validate_adjudication_receipt` | `SelectedParcels` contains an id not in `candidate_parcel_ids`, or `truth_plane != HumanAdjudication` | refusal | T22 |
| `card_artifact_mismatch` | `card::build_evidence_card` | composition, evidence, and explanation digests do not reference one another | refusal | T24 |
| `ledger_truth_plane_pooled` | `ledger::roll_up_deal`, `ledger::validate_ledger` | a rollup count or row would combine rows across `GeoTruthPlane` values without a per-plane label (I09) | refusal | T07 |
| `ledger_reach_none` | `ledger::build_ledger_row` | `reach == GeoCandidateReachStatus::None`; row emitted with `reach_none_reason` and both sets `None` (I08) | abstention | T07 |
| `ledger_sets_without_artifacts` | `ledger::build_ledger_row` | `reach != None` but `composition` or `evidence` is absent | refusal | T23 |
| `retry_pass_ceiling` | `retry::next_retry_pass` | `passes.len() == max_passes` and the latest run still abstains | abstention: `terminal = AbstainedAtCeiling`, last `abstention_reason` preserved (I10) | T10 |
| `retry_policy_unbounded` | `retry::record_pass`, schema validation | `max_passes == 0` or absent | refusal | T10 |
| `exposure_advisory_stale` | `exposure::join_exposure` | a `source_blake3s` entry is missing from `archive_blake3s`, or a later `advisory_number` for the same `storm_id` exists in the archive | refusal | T08 |
| `exposure_geometry_missing` | `exposure::join_exposure` | a ledger building has no `GeoCanonicalPolygonMm`; centroid or point input offered instead (I11) | abstention per building: id listed in `buildings_without_geometry`; refusal when every building lacks geometry | T08 |
| `collision_pari_passu_labeled` | `collision::find_collisions` | a shared entity is covered by a `GeoPariPassuDeclaration` for every colliding accession | not an error: the row is kept with `pari_passu = true` and `explanation` (I12); listed here so no implementer suppresses it | T09 |
| `inspect_artifact_missing` | `inspect::inspect` | an `output_refs` entry or receipt is absent from the work directory or fails its digest | refusal | T12 |
| `inspect_question_unanswerable` | `inspect::inspect` | a question has no artifact that answers it in this run | abstention: `answer` states the missing artifact, `artifact_refs` names the run manifest (I13) | T25 |
| `next_evidence_no_loss_model` | `next_evidence::recommend` | caller requests a total ranking and `policy` is `None` or declares no loss model | abstention on the ranking only: `total_ranking = None`, frontier still emitted (I14) | T18 |
| `condo_confirmation_insufficient` | `condo::bridge_condo_unit` | only `KeyOnly` or `BlockOnly` support exists | abstention: `confirmation` set, `billing_bbl = None`, `bins` empty, `abstained_reason` set | T11 |

### 19.5 Staged sequence and gates

Each stage names its owner bead. A gate is a frozen test or a recorded measurement with a
declared denominator. Stages may run in parallel where dependencies allow.

| Stage | Work | Gate | Owner |
|---|---|---|---|
| D0 | Truth plane rebuild: lender/party discrimination, adjudication crops as second source, 79 genuine cases or a recorded shortfall | G0: E4 denominator reaches 79 genuine subjects or the shortfall is documented with the exhausted admission rules | bd-179b, bd-7bcp |
| D1 | Evidence stacking onto the H.7 cohort: PAD address sets, asserted size bands, footprints, deed-independent observations | G1: the ignored E4 gate reports `evidence_no_observation` on zero cases; residual sizes, backbone accuracy, false merges, and abstentions recorded per truth plane | bd-1g4x, bd-1l4r |
| D2 | Propagators and explanation artifacts | G2: I01 to I05 hold on every E4 fixture; at least one real cohort conflict yields a minimal core naming source records | new beads |
| D3 | Ledger output surface, event exposure join, cross-deal collision | G3: one full public deal materializes as ledger rows with claim classes; exposure runs against one archived advisory; collision runs across two deals | new beads, bd-kwmc, bd-67wx |
| D4 | Retry loop, condo bridge, `geo inspect` | G4: recovery rate of the retry loop measured on the 40 gross-class points with fresh geocodes; condo flips independently confirmed on the 31 points; inspect answers the eight questions in architecture §1 from artifacts alone | new bead, bd-2fed, bd-1g18 |
| D5 | Next evidence controller | G5: on the D1 residuals, the controller names a nondominated action per unresolved case and a stop for every forced case | bd-vojr |
| D6 | Observer lane: NYC ortho pinning, one rule-based observer (footprint outline from a landed footprint plane as the null observer), one frozen-weight count observer, adjudication crops, evidence card | G6: observer error characterized on a named NYC population; observations admitted through rho on the D1 cohort change at least one residual or are shown redundant; card artifact validates | new beads, bd-101v |
| D7 | E5 Franklin County: deed-grain truth, typical-county tier, minimal tier | G7: tier curve recorded with abstention per tier; no Franklin-specific name in generic modules | bd-s07o, bd-3mo1 |
| D8 | Accretion and parallel protocol | G8: a new source release invalidates only the rows in architecture §8; two agents converge on one manifest | bd-2rf9, bd-3oj1 |
| D9 | Deferred-trigger review | G9: each DEFERRED item's trigger checked against D1 to D7 measurements and recorded | new P4 placeholder beads |

### 19.6 Test matrix

Columns. `Fixture or input` names the checked-in file under `tests/fixtures/geo/`, the
Demo 0 artifact under `scripts/geo_demo/demo0.sh` (`--work-dir` names below), or the
test-local constructor in an existing test file. `Assertion` names the exact field and
value. `Contracts / threats` cites §19.9 rows. `Error code` cites §19.4; `none` means the
positive path emits no code. Ids T01 to T18 are stable; T19 to T27 were added in pass 2 to
close every §19.4 row and every §19.9 threat.

| ID | Stage | Kind | Fixture or input | Assertion | Negative case | Contracts / threats | Error code |
|---|---|---|---|---|---|---|---|
| T01 | D2 | unit | universe `p1`, `p2`, `p3` built with `universe()` from `tests/geo_evidence_compilation.rs`; `IntegerSumBand` on `semantic_id "fixture:computed-area"` with contributions `p1=100`, `p2=200`, `p3=5000`, band `[5000, 5200]` | `prunings == [{ member: p3, value: Forced, propagator: AdditiveBand, constraint_ids: [band id] }]`; `fixpoint_reached == true` | `p1` is not `Excluded` (`p3 + p1 = 5100` is inside the band); an implementation that prunes it fails | C01, C02; TH01 | none |
| T02 | D2 | unit | Demo 0 `case4-warehouse-rows.json` universe (7 parcels `1004540041` to `1004540047`, buildings `1006494` to `1006500`, one building per parcel); `Cardinality` at building level `min 6, max 6` plus `Forbid` parcel `1004540047` | `1006500` is `Excluded` with `constraint_ids` containing both the cardinality id and the forbid id; the other six buildings are `Forced` | a count that ignores `parcel_ids` incidence forces `1006500` too; that output fails | C01, C02; TH01 | none |
| T03 | D2 | property | all six `e4_worked_cases.json` requests, the Demo 0 `negative-hard-chimera-composition.json`, and 200 seeded iterations of `random_constraints` from `tests/geo_composition.rs` | `check_soundness(...).sound == true` and `differing_models.is_empty()` on every input; `model_count_before == model_count_after` | a test-local propagator that unconditionally `Excluded`s the first universe member returns `sound == false` and `Err` code `propagation_unsound_detected` with `detail["member"]` set | C02; TH01 | `propagation_unsound_detected` |
| T04 | D2 | property | same inputs as T03; all six permutations of `[AdditiveBand, Cardinality, SourceExclusivity]` | `canonical_propagation_bytes` identical across permutations; `rounds` may differ, `prunings` may not | a permutation-dependent `prunings` order is caught by the byte compare | C03; TH02 | none |
| T05 | D2 | unit | Demo 0 `negative-hard-chimera-composition.json` (constraint ids `asserted_address_core`, `area_majority_buildings`, `chimera_wrongly_admitted`) with an evidence compilation mapping each id to one `source_record_id`; `GeoReliabilityOrder` `[asserted_address_core, area_majority_buildings, chimera_wrongly_admitted]` | `cores[0].constraint_ids == ["asserted_address_core", "chimera_wrongly_admitted"]`, `cores[0].minimal == true`, `cores[0].source_record_ids.len() == 2`, no `constraint_ids` entry is numeric | a core that also lists `area_majority_buildings` fails the I03 re-solve and returns `core_not_minimal` with `detail["constraint_id"] = "area_majority_buildings"` | C05, C07; TH04, TH06 | `core_not_minimal` |
| T06 | D2 | unit | T05 input; `GeoExplanationBudget { max_core_solves: 64, max_cores: 8, max_hitting_sets: 8 }` then the same with `max_cores: 1` | with `max_cores: 8`: every `correction_sets[i].observation_ids` intersects every `cores[j].observation_ids`, `cores_complete == true`; with `max_cores: 1`: `cores_complete == false`, `explanation_complete == false`, every `minimal == false` | a correction set that misses one enumerated core fails the intersection check | C06; TH05 | `core_enumeration_ceiling` |
| T07 | D3 | e2e | the 15 `e4_gate_v2_population.json` cases as loans of one synthetic accession, `loan_id` from the `loan_key` of the companion `e4_gate_v2_evidence_enrichment.json` (case `3cf11e9a58e3b710` maps to `073ad3a0862827c75501ac66570eb783`); one case forced to `GeoCandidateReachStatus::None` with reason `"no_candidate_parcels"` | `rollups[0].rows == 15`; per-plane `resolved + ambiguous + conflict + reach_none` sums to `rows`; `GeoDealRollup` serializes no field named `total`; the reach-none row has `parcel_set == None`, `building_set == None`, `reach_none_reason == Some("no_candidate_parcels")` | `roll_up_deal` over rows of two truth planes without per-plane labels refuses `ledger_truth_plane_pooled` | C13, C14; TH12 | `ledger_truth_plane_pooled`, `ledger_reach_none` |
| T08 | D3 | e2e | ledger from Demo 0 case 4 (buildings `1006494` to `1006499`); advisory fixture `tests/fixtures/geo/advisory_synthetic_adv12.json` (shipped by the D3 bead) with 34, 50, and 64 knot square rings placed so `1006494` lies inside the 64 knot ring and `1006499` lies outside all rings; `archive_blake3s` containing every `source_blake3s` entry | `exposed` contains `{ building_id: "1006494", knots_band: 64 }` and no entry for `1006499`; `advisory.source_blake3s` is nonempty and equals the fixture pins | a `geometry` map that supplies a point for every building refuses `exposure_geometry_missing`; an `archive_blake3s` missing one pin refuses `exposure_advisory_stale` | C15; TH13, TH23 | `exposure_advisory_stale`, `exposure_geometry_missing` |
| T09 | D3 | e2e | two ledgers whose rows share parcel `1004540041` and parcel `1004540042`; one `GeoPariPassuDeclaration` for `1004540041` only | `collisions` has exactly two `SharedParcel` rows; the `1004540041` row has `pari_passu == true` and a nonempty `explanation`; the `1004540042` row has `pari_passu == false`; both carry both accessions | an implementation that drops the declared row leaves one collision; that output fails | C16; TH14 | `collision_pari_passu_labeled` |
| T10 | D4 | e2e | `GeoRetryPolicy { max_passes: 2, .. }`; two `GeoRun` values whose `status` abstains with `blockers` naming `geocode_ambiguous` | after two `record_pass` calls: `passes.len() == 2`, `terminal == Some(AbstainedAtCeiling)`, `passes[1].abstention_reason == "geocode_ambiguous"`, every pass has `plan_blake3` and `run_blake3` set; `next_retry_pass` returns `Ok(None)` | `max_passes: 0` refuses `retry_policy_unbounded` with `detail["field"] = "max_passes"` | C17; TH15 | `retry_pass_ceiling`, `retry_policy_unbounded` |
| T11 | D4 | unit | `GeoCondoBridgeRequest` where `billing_bbl_candidates == [pip lot]`, `block` matches, and the footprint ring has majority area outside the parcel ring | `confirmation == BlockOnly`, `billing_bbl == None`, `bins.is_empty()`, `abstained_reason.is_some()` | with majority area inside: `confirmation == BlockAndGeometry`, `billing_bbl == Some(pip lot)`, `bins.len() == 1` | C20; TH18 | `condo_confirmation_insufficient` |
| T12 | D4 | e2e | Demo 0 work directory after `bash scripts/geo_demo/demo0.sh --work-dir <dir>` | `answers.len() == 8`; every `artifact_refs.len() >= 1`; `inspect` has no call path to `solve_composition` (checked by the T27 literal scan on `inspect.rs`) | delete `solve.json` from the work directory: `inspect` refuses `inspect_artifact_missing` with `detail["artifact"] = "solve.json"` | C18; TH16 | `inspect_artifact_missing` |
| T13 | D6 | unit | three `GeoObserverContract` and pin variants: `FrozenWeight` with empty `weight_blake3`; `characterization_blake3 == ""`; a tile pin with `license_id = "commercial_basemap_tos"` listed in `forbidden_license_ids` | each `admit_observations` call returns `Err` with the matching code and `detail["field"]` naming the missing or forbidden field | a complete contract and a CC BY pin admit with `not_admitted_ids.is_empty()` | C09; TH09, TH10 | `observer_missing_provenance`, `observer_error_uncharacterized`, `observer_license_forbidden` |
| T14 | D6 | e2e | a `GeoObservationRowsArtifact` with one `StructureCountInWindow` row and a `bytes_by_blake3` map holding the pinned tile, crop, and label bytes | `verify_replay` returns `Ok(())`; the T27 literal scan finds no identity invocation in `observer.rs` replay path | flipping one byte of the tile refuses `image_tile_digest_mismatch` with `detail["tile"]`; a row whose `label_blake3` differs from the stored artifact refuses `observation_regenerated_at_replay` with `detail["observation_id"]` | C10; TH08 | `image_tile_digest_mismatch`, `observation_regenerated_at_replay` |
| T15 | D6 | unit | a `PresentAtVintage` row over the universe of `temporal_occupancy_cannot_be_smuggled_in_as_timeless_property_identity` in `tests/geo_evidence_compilation.rs` | `to_rho_observation` returns `Some` with `valid_time == Some(interval)`; the row id is in `diagnostic_only_ids`; the compiled request has `hard_constraints.is_empty()` and `residual_model_count == 3` | an implementation that emits a `Require` from the vintage observation changes `residual_model_count` and fails | C11; TH10 | `observation_temporal_diagnostic` |
| T16 | D7 | e2e | E5 Franklin measurement run artifact from bd-s07o | the tier curve artifact carries `abstention_cases` per tier with a declared denominator per tier; T27 passes on every generic module | a tier row without a denominator fails validation | C22, C23; TH20, TH21 | none |
| T17 | D1 | gate | `cargo test --test geo_adjudication e4_acceptance_gate_requires_the_full_population_to_be_reachable -- --ignored` over `e4_gate_v2_population.json` (15 cases) with `frozen_e4_h7_gate()` | `evidence_no_observation_cases == 0` | the gate thresholds are constants inside the test; a change under `src/` cannot move them (N11) | C24; TH19 | none |
| T18 | D5 | unit | candidates `A { cost_units: 1, worst_case_remaining: 2 }` and `B { cost_units: 2, worst_case_remaining: 2 }`; `policy == None` | `frontier == [A]`, `dominated == [B]` with `B.dominated_by == ["A"]`, `total_ranking == None`, `stop == None` | a total ranking emitted with `policy == None` fails; with a policy declaring a loss model `total_ranking == Some(["A", "B"])` | C19; TH17 | `next_evidence_no_loss_model` |
| T19 | D2 | unit | one 200-iteration `random_constraints` component from `tests/geo_composition.rs` with `GeoPropagationBudget { max_fixpoint_rounds: 1, max_hall_subset_size: 1, max_subset_sum_states: 1 }` | `fixpoint_reached == false`; `budget_fallback.is_some()` and `budget_fallback.counter` is one of the three budget field names with `configured == 1`; `check_soundness` on the retained `prunings` still returns `sound == true`; exit is zero | `max_fixpoint_rounds: 0` refuses `invalid_input` with `detail["field"] = "max_fixpoint_rounds"` (mirrors `zero_assignment_budget_refuses_validation`) | C04, C02; TH03 | `propagation_budget_exhausted` |
| T20 | D2 | unit | `e4_worked_cases.json` `case_1_clean_rooftop` request (expected `status: resolved`, `residual_model_count: 1`) | `minimal_core` returns `Err` code `explanation_not_conflict` with `detail["status"] = "resolved"`; no artifact emitted | `case_6_dense_one_parcel_multi_building` (`status: ambiguous`, count 2) also refuses with `detail["status"] = "ambiguous"`; the Demo 0 chimera request does not refuse | C05; TH04 | `explanation_not_conflict` |
| T21 | D2 | unit | the `global_mask_overflow_now_solves_exactly` request from `tests/geo_composition.rs` with `max_assignments` lowered until the baseline reports `residual_model_count_complete == false` or `residual_model_count_saturated == true`; one `GeoProspectiveObservation` with two outcomes | every `per_outcome[*].count_exact == false`, `redundant == false`, `baseline_model_count` equals the baseline artifact count; no field named `expected_value` serializes | on `case_6_dense_one_parcel_multi_building` with outcomes inducing `Require` building `1076314` and `Require` building `1085187`: `per_outcome` counts `[1, 1]`, `count_exact == true`, `worst_case_remaining == 1` | C08; TH07 | `separation_residual_inexact` |
| T22 | D6 | unit | `GeoAdjudicationRequest` with `candidate_parcel_ids == ["1004540041", "1004540042"]`; receipt `SelectedParcels(["1004540099"])`; second receipt with `truth_plane` set to any `GeoTruthPlane` other than `HumanAdjudication` | both `validate_adjudication_receipt` calls return `Err` code `adjudication_label_outside_candidates`; the first has `detail["parcel_id"] = "1004540099"`, the second has `detail["truth_plane"]` | `SelectedParcels(["1004540041"])` with matching `request_blake3` and `crop_blake3` returns `Ok(())` | C12; TH11 | `adjudication_label_outside_candidates` |
| T23 | D3 | unit | `build_ledger_row` with `reach == Full`, `composition == None`, `evidence == Some(..)` | `Err` code `ledger_sets_without_artifacts` with `detail["field"] = "composition"`; a second call with `evidence == None` names `"evidence"` | `reach == None` with `reach_none_reason == None` refuses `invalid_input` with `detail["field"] = "reach_none_reason"`; `reach == None` with a reason emits a row with both sets `None` (`ledger_reach_none`) | C13; TH22 | `ledger_sets_without_artifacts` |
| T24 | D6 | unit | Demo 0 `solve.json` as composition, Demo 0 `evidence-compilation.json` as evidence, and an explanation artifact whose `evidence_blake3` is the digest of `negative-hard-chimera.json` instead | `build_evidence_card` returns `Err` code `card_artifact_mismatch` with `detail["evidence_blake3"]` naming both digests | with the explanation's `evidence_blake3` matching: `Ok`, `forced.parcels` equals the six case 4 parcels, `ambiguous_members.is_empty()`, `composition_blake3` equals the `solve.json` digest | C21; TH22 | `card_artifact_mismatch` |
| T25 | D4 | unit | a work directory holding the run manifest and `solve.json` only (no explanation artifact) | `inspect` returns `Ok`; `answers.len() == 8`; the answer for the question that only a `GeoExplanationArtifact` answers has `answer` naming `canon_geo_explanation.v0` as missing and `artifact_refs == [run manifest ref]`; exit is zero | an answer with empty `artifact_refs` fails `validate_inspection_artifact`; `inspect` still refuses `inspect_artifact_missing` when a referenced artifact is absent (T12) | C18; TH16 | `inspect_question_unanswerable` |
| T26 | D3, D6 | adversarial | every artifact written by `scripts/geo_demo/demo0.sh` | `evidence-compilation.json`: every `admissions[].contract.source_dataset` starts with `fixture.`; no Demo 0 artifact is read by `frozen_e4_h7_gate()` or by any G-gate test; a ledger row built from Demo 0 carries `source_release_pins[].source_dataset` starting with `fixture.` | a ledger row from fixture input whose pins are relabeled to a live dataset name fails `validate_ledger` | C23; TH20 | none |
| T27 | D2 to D7 | adversarial | the twelve generic module files listed in §19.3 | a case-insensitive scan finds none of the literals `1004540041`, `chimera_wrongly_admitted`, `asserted_address_core`, `case_4`, `franklin`, `solve_composition` (the last only in `inspect.rs`), or any hosted-model client symbol in `observer.rs` replay code | the scan must also match `Franklin` and `CASE_4`; a scan that is case-sensitive fails on a seeded scratch copy | C18, C25; TH16, TH21 | none |

### 19.7 Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test geo_adjudication -- --ignored   # frozen E4 gate, expected red until G1
bash scripts/geo_demo/demo0.sh --work-dir /tmp/demo0
cargo run --bin canon -- geo capabilities --emit json
cargo run --bin canon_geo_measurements -- --emit plan
br ready --json && br dep cycles --json
```

### 19.8 Bead map

Existing beads retained with their stage: bd-179b (D0), bd-7bcp (D0), bd-1g4x (D1),
bd-1l4r (D1), bd-kwmc and bd-67wx (D3), bd-2fed and bd-1g18 (D4), bd-vojr (D5), bd-101v
(D6), bd-s07o and bd-3mo1 (D7), bd-2rf9 and bd-3oj1 (D8), bd-2b9d and bd-2cbs and bd-1wpv
(candidate reach and levels, feeding D1), bd-29cf (citation audit, chore lane).

New beads created 2026-09-01 for D2, D3, D4, D6, and D9 carry the full design text from
§18 and §19 so that no implementer needs to return to this document.

### 19.9 Contracts and threats

Added in pass 2. Contracts are the behavioral rows an implementer must satisfy; each cites
the invariant or non-negotiable it enforces and the module that owns it. Threats are the
ways a naive or reward-hacking implementation fails; each names the contract and the test
that catches it. Numbering is stable; add rows at the end.

**Contracts**

| Id | Statement | Module | Enforces |
|---|---|---|---|
| C01 | Every `GeoPruning` carries at least one `constraint_ids` entry and, when evidence is supplied, the `evidence_ids` of the observations that justify it; a pruning with empty `constraint_ids` fails validation | `propagate.rs` | I01, I05 |
| C02 | `check_soundness` compares `residual_models` and `summary.residual_model_count` between the original and narrowed request and reports `sound == false` on any difference; propagation never changes the model set | `propagate.rs` | I01, N04 |
| C03 | The propagation fixpoint is order-independent: `canonical_propagation_bytes` is identical under every permutation of `GeoPropagatorKind`; no `HashMap` iteration reaches an artifact | `propagate.rs` | I02, N02 |
| C04 | Budget exhaustion is a typed fallback, never a silent fixpoint: `fixpoint_reached == false`, `budget_fallback` names the counter and configured value, retained prunings remain individually sound | `propagate.rs` | I01, §19.4 typed fallback class |
| C05 | `minimal_core` refuses unless the input solves to `Conflict`, and every emitted core with `minimal == true` has been verified by re-solving each single deletion | `explain.rs` | I03 |
| C06 | Every emitted correction set hits every enumerated core; when `max_cores` or `max_core_solves` is reached, `cores_complete == false`, `explanation_complete == false`, and no `minimal` flag is `true` | `explain.rs` | I04 |
| C07 | Cores and correction sets name `source_record_ids` and `rho_contract_ids`; constraint ids appear only as declared ids, never as positional indices | `explain.rs` | I05, §18.5 item 2 |
| C08 | `separate` reports one exact count per declared outcome and sets `count_exact == false` whenever the baseline is incomplete or saturated or an outcome solve falls back; no artifact field carries an expected value or a probability | `explain.rs` | §18.5 item 3, N05 |
| C09 | `admit_observations` refuses any contract, row, or pin missing an I06 field, any `kind` outside `output_kinds`, any empty `error_population_id` or `characterization_blake3`, and any pin whose `license_id` is forbidden or whose `license_text_blake3` is empty | `observer.rs` | I06, N09, N10 |
| C10 | `verify_replay` recomputes tile, crop, and label digests from supplied bytes and contains no call path to any `GeoObserverIdentity`; a differing digest refuses | `observer.rs` | I07, N02 |
| C11 | An observation enters the solver only through `to_rho_observation` as a `GeoRhoObservation` under a declared `rho_contract_ids` entry; `PresentAtVintage` and `AbsentAtVintage` are listed in `diagnostic_only_ids` and induce no hard constraint until dated composition exists | `observer.rs` | N03, §18.4 kinds table |
| C12 | `validate_adjudication_receipt` accepts only `truth_plane == HumanAdjudication`, a `request_blake3` and `crop_blake3` that match, and `SelectedParcels` drawn from `candidate_parcel_ids` | `adjudicate.rs` | §18.4 first use, D0 truth plane |
| C13 | A ledger row with `reach == None` carries `reach_none_reason` and no sets; any other reach requires both the composition and the evidence artifact and binds accession, loan id, parcel set, building set, truth plane, claim class, model count, exactness flags, and every source release pin | `ledger.rs` | I08 |
| C14 | `GeoDealRollup` counts are keyed per `GeoTruthPlane`; no pooled total exists on the type, and `roll_up_deal` refuses rows that would pool planes | `ledger.rs` | I09, N06 |
| C15 | `join_exposure` tests building polygons against wind radius rings by exact geometry, refuses centroid or point input, requires every `source_blake3s` entry in `archive_blake3s`, and refuses a superseded `advisory_number` | `exposure.rs` | I11 |
| C16 | `find_collisions` reports every parcel or building shared by more than one accession with all accessions and loan ids; a declared pari passu match sets `pari_passu == true` with an `explanation` and the row is kept | `collision.rs` | I12 |
| C17 | The retry loop is bounded by `max_passes > 0`; every pass records `plan_blake3`, `run_blake3`, and `abstention_reason`; `next_retry_pass` emits acquisition requests and never geocodes | `retry.rs` | I10 |
| C18 | `inspect` reads artifacts and receipts by digest, has no call path to `solve_composition`, emits exactly eight answers each with at least one `artifact_refs` entry, and types an unanswerable question as an abstention | `inspect.rs` | I13 |
| C19 | `recommend` always emits the nondominated `frontier`; `total_ranking` is `Some` only when `policy` declares a loss model; dominance uses cost and per-outcome separation only | `next_evidence.rs` | I14 |
| C20 | `bridge_condo_unit` emits `billing_bbl` and `bins` only under `BlockAndGeometry`; `BlockOnly` and `KeyOnly` abstain with empty sets and an `abstained_reason` | `condo.rs` | bd-2fed, T11 |
| C21 | `build_evidence_card` refuses when the composition, evidence, and explanation digests do not reference one another; `ambiguous_members` is the universe minus the backbone minus members absent from every residual model | `card.rs` | §18.4 second use |
| C22 | Every artifact type declares `CANON_GEO_<NAME>_VERSION`, `canonical_<name>_bytes`, `validate_<name>_artifact`, and a `schemas/canon.geo.<name>.v0.schema.json`; canonical bytes are byte-identical across platforms | all §19.3 modules | N02, §19.3 shared shape |
| C23 | Artifacts built from `tests/fixtures/geo/` or `scripts/geo_demo/` inputs carry `source_dataset` pins beginning `fixture.` and are never read by a G-gate test or cited as a measurement | all §19.3 modules | N07 |
| C24 | Gate thresholds (`frozen_e4_h7_gate()` and the G0 to G9 conditions in §19.5) live in tests and this document; code under `src/` cannot move them, and a gate passes only by meeting it | `tests/geo_adjudication.rs`, §19.5 | N11 |
| C25 | D2 to D6 do not modify `src/geo/composition.rs` or `src/geo/evidence.rs`; new modules call `solve_composition` as a black box, and generic modules carry no fixture, demo, or county literal | all §19.3 modules | §19.3 dependency direction, G7 |

**Threats**

| Id | Threat | Mitigation | Covered by |
|---|---|---|---|
| TH01 | A propagator over-prunes and silently removes a true model (unsound domain filter presented as exact) | Soundness check on every fixture and seeded random component; unsound result is a refusal and a CI failure | C02; T03, T01, T02; `propagation_unsound_detected` |
| TH02 | Propagator output depends on propagator order or on `HashMap` iteration, so two platforms emit different bytes | Permutation byte compare; `BTreeMap` only; sorted `prunings` | C03; T04 |
| TH03 | Budget exhaustion reported as a reached fixpoint, or prunings dropped without a fallback record | Typed fallback with counter name and configured value; retained prunings re-checked for soundness | C04; T19; `propagation_budget_exhausted` |
| TH04 | A superset core, or a core computed on a satisfiable input, presented as minimal | Deletion re-solve per member; refusal on non-conflict input | C05; T05, T20; `core_not_minimal`, `explanation_not_conflict` |
| TH05 | Minimality or completeness claimed after the core enumeration ceiling | `cores_complete` and `explanation_complete` forced `false`, every `minimal` forced `false` at the ceiling | C06; T06; `core_enumeration_ceiling` |
| TH06 | Explanation names internal constraint indices, so the artifact cannot be sent upstream to a source owner | Cores and correction sets carry `source_record_ids` and `rho_contract_ids` | C07; T05 |
| TH07 | Counterfactual separation smuggles in an expected value, a probability, or a `redundant` claim on an inexact count | `count_exact` gating; no expected-value field on the type | C08; T21; `separation_residual_inexact` |
| TH08 | A hosted or frozen model is re-run at replay and its fresh output replaces the stored observation | `verify_replay` has no identity call path; digest mismatch refuses | C10; T14; `observation_regenerated_at_replay`, `image_tile_digest_mismatch` |
| TH09 | A commercial basemap tile enters through a pin with a missing or mislabeled license | License gate on `license_id` and `license_text_blake3`; forbidden list is an input, never a constant a module can drop | C09; T13; `observer_license_forbidden` |
| TH10 | A vision model proposes a location, or an uncharacterized observer's output reaches the solver as a hard constraint | Observations enter only through `rho` under a characterized error population; vintage kinds stay diagnostic | C09, C11; T13, T15; `observer_error_uncharacterized`, `observation_temporal_diagnostic` |
| TH11 | An adjudicator labels a parcel outside the shown candidates, or a non-human truth plane is written as human adjudication | Receipt validation against `candidate_parcel_ids` and `truth_plane` | C12; T22; `adjudication_label_outside_candidates` |
| TH12 | Truth planes pooled in a rollup so the resolved rate reads higher than any single plane supports | Per-plane rollup type with no total; pooling refuses | C14; T07; `ledger_truth_plane_pooled` |
| TH13 | Event exposure computed on centroids or address points, so a building on the ring edge is misclassified | Polygon-in-ring by exact geometry; point input refuses; missing geometry is listed, never guessed | C15; T08; `exposure_geometry_missing` |
| TH14 | A pari passu collision suppressed as a false positive, or an undeclared shared parcel hidden | Row kept with `pari_passu == true` and an explanation; undeclared rows flagged | C16; T09; `collision_pari_passu_labeled` |
| TH15 | The retry loop runs unbounded or geocodes inside the loop, breaking pinned reproducibility | `max_passes > 0` required; each pass is a normal pinned run; loop emits requests only | C17; T10; `retry_policy_unbounded`, `retry_pass_ceiling` |
| TH16 | `geo inspect` recomputes a solve and its answer diverges from the emitted artifact | Read-only by digest; no `solve_composition` call path; literal scan | C18; T12, T25, T27; `inspect_artifact_missing`, `inspect_question_unanswerable` |
| TH17 | A total ranking of next actions manufactured without a loss model | `total_ranking` gated on a declared loss model; frontier always emitted | C19; T18; `next_evidence_no_loss_model` |
| TH18 | Condo unit confirmed by key equality alone (billing BBL equals PIP lot) | Block match plus majority-area footprint containment required | C20; T11; `condo_confirmation_insufficient` |
| TH19 | Gate self-weakening: a frozen gate, threshold, or denominator edited so a red test passes | Gates are constants in tests and in §19.5; conformance lane is single-owner with reviewer sign-off per AGENTS.md | C24, N11; T17 |
| TH20 | Proof-class inflation: Demo 0 output, fixtures, or retained evidence cited as a live measurement or gate pass | `fixture.` pins on every fixture-derived artifact; G-gate tests read only their frozen fixtures; measurement receipts require runtime-selected subjects with a recorded seed | C23, N07; T26, T16 |
| TH21 | Demo-path hard-coding: a generic module special-cases Demo 0 ids, the chimera constraint ids, or Franklin County names to pass its tests | Case-insensitive literal scan over the twelve generic modules; property tests on seeded random components | C25; T27, T03, T16 |
| TH22 | Fabricated sets: a ledger row or evidence card emitted without the composition and evidence artifacts that justify it, or with digests that do not chain | Both artifacts required for any reach other than `None`; card digests must reference one another | C13, C21; T23, T24; `ledger_sets_without_artifacts`, `card_artifact_mismatch` |
| TH23 | A stale advisory joined after a later advisory number exists in the archive | Latest-advisory check per `storm_id`; every source hash must be in the archive | C15; T08; `exposure_advisory_stale` |

---

# Appendix A — Frozen-weight observers and imagery sources

Added 2026-08-15 after operator correction. **This appendix is an open extension point, not
a commitment.** It records what the architecture does and does not forbid, and exactly what
admission requires.

## A.1 The correction: neural networks are not banned. Nondeterminism is.

The main plan's constraint is byte-identical reproducibility, not an aversion to models. A
model with **frozen, hashed, versioned weights, implemented in pure Rust with controlled
arithmetic** — no platform-variable BLAS, no nondeterministic kernels, no sampling — is
byte-identical forever and satisfies the determinism requirement exactly as well as an
integer orientation predicate does.

CMD+RVL has shipped this pattern three times: **cmdrvl-tabfm**, **FrankenWhisper**, and
**FrankenOCR** (with the `focr` skill on the operator's machine). The
`ai-model-into-rust-mega-fused-hyper-kernel` skill states the target explicitly as
**bit-identical model parity**.

A frozen model's version therefore becomes another pinned input in the artifact chain,
exactly like the registry version, the strategy hash or the tile digest.

## A.2 The rule that keeps it sound: a model is a SOURCE, not a SOLVER

canon's explainability rule — *if you cannot explain a match by pointing at assertion
scores, it does not ship* — constrains **what the model outputs**, not whether one exists.

| Model output | Admissible? |
|---|---|
| "probably the right building, 0.87" | **No.** Opaque scalar, not decomposable, decides nothing legibly |
| "3 structures inside this polygon" | **Yes** — a count, checkable against `NUMBLDGS`, enters as a constraint |
| "roof outline at these coordinates" | **Yes** — a geometry, enters ρ like any other observed footprint |
| "12 floors from facade or shadow" | **Yes** — a number, checkable against `NUMFLOORS` |

> **The model observes. The constraint system decides.** A frozen-weight observer is a
> sixth source with its own ρ band; nothing else in the architecture changes.

## A.3 The admission gate: you must characterize the error before you can write ρ

ρ requires the *weakest constraint the source can support*. For an integer predicate the
error is zero. For an observer it must be **measured**, e.g. *"structure count correct
within ±1 at 94% on NYC-density blocks, degrading to 78% under closed canopy."*

That measurement is real work and it precedes admission. It is also not global: **error
varies by land cover, density, season and imagery vintage**, so the band is regional, not a
single constant. No characterization, no admission.

## A.4 What an observer could supply that no landed source does

- **Building count per parcel** — `NUMBLDGS` is unreliable; this checks it independently
- **A footprint we generate ourselves** — a geometry source we license from nobody
- **Construction state — is there a building here *now*?** The Allen interval constraints in
  §7 need demolition and new-construction observations and currently have **no observation
  source at all**
- **Confirmation of roof-ridge over-segmentation.** Régin's Hall-set violation *proves*
  a source split one building into several; imagery *shows* it. Proof and picture in the
  same evidence card.

## A.5 Imagery and elevation source inventory

**Verified 2026-08-15:**

| Source | Type | Res | License | Access |
|---|---|---|---|---|
| **NAIP** (USDA FPAC-BC) | aerial, RGB+NIR | 1 m 2003–17; **0.6 m from 2018**; 0.3 m coastal option | **Public domain** (US Gov work) | Three public S3 buckets — `naip-visualization` (3-band **COG**), `naip-analytic` (4-band MRF), `naip-source` (raw GeoTIFF). STAC catalogs exist (`stactools-naip`, Planetary Computer). |

**NAIP access pattern — the one to use.** COG plus **HTTP range requests**: fetch only the
byte window covering an H3 cell. No bulk download, no tile server, no API key, no rate
limit. **Pin the S3 object version or ETag and imagery becomes a content-addressed input**
like the tile artifact itself; a byte-range read is deterministic.

**NAIP caveats, all real:**
1. **Vintage varies by state and is not annual** — rotating 2–3 year schedule. Imagery date
   is a per-tile fact and must enter the temporal constraints as an *observation date*,
   never as "now."
2. **Recent coverage needs verifying.** The AWS registry describes 2011–2018 for one bucket
   while STAC catalogs claim 2010–present. Confirm against the live bucket before planning.
3. **Leaf-on growing-season imagery.** Canopy obscures structures in tree-heavy areas —
   irrelevant for dense Manhattan, material for suburban and agency multifamily. This is
   the dominant term in the ρ band and it is seasonal and geographic.

**Not yet verified — candidates to assess (claims below are from general knowledge and
MUST be checked before any commitment):**

| Candidate | Why it might matter |
|---|---|
| **USGS 3DEP LiDAR** | **Possibly better than imagery for this problem.** Height is *measured*, not inferred; building extraction from point clouds is classical geometry rather than ML, so there may be **no model to characterize at all**; distinguishes building from parking lot trivially; yields floor-count estimates directly against `NUMFLOORS`. Public domain, on AWS Open Data. Coverage incomplete and vintage varies widely. **Assess this before assuming imagery is the right sensor.** |
| **Sentinel-1 SAR** (ESA) | Radar, ~10 m, all-weather, free and open. Too coarse for footprints but buildings have distinctive double-bounce signatures — potentially useful for *presence and change detection* rather than geometry. |
| **Sentinel-2** (ESA) | ~10 m optical, free and open. Too coarse for individual buildings; useful for change detection and land cover. |
| **State and municipal orthoimagery** | Several states and NYC specifically fly higher-resolution orthophotos than NAIP, often public. Best resolution available for the proving ground. |
| **USGS High Resolution Orthoimagery (HRO)** | Public domain, higher res than NAIP in covered areas. |
| **NOAA emergency-response imagery** | Post-disaster, high-res, public domain. Narrow but valuable for change events. |
| **Commercial** — Maxar, Planet, Nearmap, Vexcel, Airbus | Higher res and better cadence, **licensed**. Same containment discipline as §10; resolvable-under-licence, not redistributable. |
| **Mapbox / Google / Esri basemaps** | **Avoid.** Terms generally forbid caching and redistribution, which is incompatible with a pinned evidence artifact. |

## A.6 The generalization

> **If the frozen-weight pattern holds, any imagery source is just another source.**

The constraint kernel is largely indifferent to which sensor produced an observation, but
calibration and dependence are not. Each new source needs at least four things:

1. a **license posture** — resolvable, redistributable, or neither (§10)
2. a **vintage** per observation, feeding the temporal constraints
3. a **characterized error**, which becomes its ρ band (§A.3)
4. **upstream lineage** — imagery flight, municipal layer, model, and derived products that
   may share one error mechanism with another nominal source

Everything downstream — propagation, the residual, MUS, model counting, the certificate —
is unchanged. Adding a sensor is a data-onboarding task, not an architectural one. **That
is the property worth protecting.**

---

# Appendix B — MEASURED: the component-size claim is not supported

Added 2026-08-15. **This is the first hard measurement against real data and it falsifies a
load-bearing claim in §6.** Recorded here rather than quietly amended, because the claim was
used to justify the exact-compilation strategy and the 0.5 s/tile budget.

## B.1 What §6 claimed

> "Typical component after slot-level geometric filtering: **6–20 variables**, d ≤ 8. Tail to
> ~40 on a dense assemblage." — and therefore exact compilation is affordable and subsumes
> all k-consistency.

**This was an estimate presented without measurement.**

## B.2 The measurement

Real tile: 100 MapPLUTO parcels + 93 NYC building footprints within 150 m of the 305 E 72nd
rooftop geocode (`docs/geo_design_session/TILE_305_E_72ND.txt`). Bipartite graph, parcel
centroid ↔ footprint centroid, edge where haversine distance ≤ r. Connected components:

```
  r(m)  comps    mean   max   p50   p90  in 6-20  isolated
    10     64    3.02    17     2     7        8        16
    15     51    3.78    25     2     8        7         7
    20     33    5.85    34     2    15        7         4
    25     24    8.04    37     4    16        9         1
    30     12   16.08    49    10    37        5         1
    35      7   27.57    59    31    59        1         1
    40      6   32.17    59    37    59        1         1
    50      3   64.33    77    59    77        0         0
    60      1  193.00   193   193   193        0         0
   150      1  193.00   193   193   193        0         0
```

## B.3 The verdict: there is no usable plateau

The adversarial review predicted the exact test: *"either there is a stable plateau where
components land in 6–20, or the distribution jumps from singletons to tens with no usable r,
and the claim is dead."*

**It is the second one.** The distribution goes from mostly-singletons straight to a giant
component:

- At the most favourable radius (r = 25 m) only **9 of 24** components fall in the 6–20 band,
  and the **maximum is already 37** — above §6's stated ceiling.
- At r = 30 m the mean is 16 but the max is **49**.
- **The tile percolates at r ≈ 60 m** into a single 193-variable component, well inside the
  150 m tile radius.

**Centroid proximity does not decompose this tile at any radius.** The exact-compilation
argument in §6 rests on a decomposition that this filter does not produce.

## B.4 What actually decomposed the tile, and why it does not count

The ground-truth pass measured components via the footprint table's `MAPPLUTO_BBL → BBL`
bridge and got mean 2.92, max 5 — comfortably inside budget. **That number is vacuous, and
must not be cited as support.**

> Measuring components through `MAPPLUTO_BBL` measures the component structure of an
> equivalence relation whose classes are defined by the key being resolved. It is a fact
> about Manhattan building stock — footprints per tax lot — not about a resolution
> architecture. And if `MAPPLUTO_BBL` exists, that edge needs no propagation at all; it is a
> deterministic join. **An architecture that passes here passes by not being exercised.**

The honest entry for the component-size row is **not applicable**, not a number with an
asterisk.

## B.5 The 25 m filter, corrected

§B.2's companion measurement — equal-area disc radius `√(A/π)` per parcel, a hard floor on
centroid-to-boundary distance:

```
  median parcel   8.7 m
  p90            18.6 m
  max            33.4 m   ← BBL 1014477501, LOTAREA 37,800 — the answer parcel
  parcels exceeding 25 m:  4 / 100  =  4%
```

So the earlier refutation ("a 25 m filter deletes the true parcel, which sits 31.58 m from
its centroid") had the right conclusion for the wrong reason. **The correct statement is
worse for the filter:**

> A fixed 25 m centroid radius is adequate for 96% of parcels and fails **specifically and
> silently on the ~4% that are large assemblages** — which are precisely the hard cases this
> product exists to solve. The answer parcel here is the single largest in the tile.

**A fixed radius is the wrong shape of filter.** Any replacement must normalise by parcel
extent — half-diagonal from `LOTFRONT`/`LOTDEPTH`, or the equal-area radius above — rather
than applying a constant.

## B.6 Consequences

1. **§6's consistency ladder and 0.5 s/tile budget are unvalidated.** Do not quote them.
2. **The decomposition mechanism is an open question**, and probably needs real polygon
   containment rather than centroid proximity. That requires a re-pull with
   `ST_AsWKT(GEOM_GEOG)` on both tables.
3. **If no filter decomposes dense tiles**, exact compilation may be affordable only in
   sparser geographies, and dense urban tiles may need a different strategy — a real
   possibility that must be priced before committing.
4. **Scope of this finding: n = 1, and it is the dense extreme.** Midtown-adjacent Manhattan
   is the worst case for percolation. A suburban or agency-multifamily tile will decompose
   far more readily. **The claim is falsified for dense urban, not universally** — measure
   across a stratified tile sample before concluding anything general.

## B.7 The process lesson

The estimate survived a full red team, an adversarial cross-scoring round, and two model
families independently endorsing the architecture. **It was killed by twenty minutes of
arithmetic on data we already had.** No amount of adversarial reasoning substitutes for one
measurement.

## B.8 Fresh pinned reproduction

On 2026-08-28 the observation pull was re-run through cmdrvl-data MCP with MapPLUTO pinned
to `26v1` / `2026-05-01` and NYC footprints pinned to `2026-08-09`. It returned the same
100 parcels and 93 footprints. Recomputing the bipartite centroid graph with mean-Earth
haversine distance reproduced every row of B.2 exactly, including percolation to one
193-node component at 60 m. Snowflake query
`01c6b1c1-0821-784b-006c-c7030888c3ce`; executable query and expected block:
`scripts/geo_measurements/appendix_b_centroid_percolation.sql`.

---

# Appendix C — MEASURED: the tile is ~7× larger than the plan assumes

Added 2026-08-15, immediately after Appendix B. **This falsifies the single number the entire
commercial thesis rests on.**

## C.1 What the plan and the epic assume

> §6: "About 200 features per tile, so methods that are globally intractable are often FREE
> here… **O(n³) is free. Even O(2ⁿ) over a filtered subset can be free.**"
>
> bd-2kjx: the work unit is "that cell **plus its 6 neighbours** (7 cells, 0.737 km²) so
> boundary buildings resolve."

## C.2 The measurement

Feature density across **all 1,192 H3 r8 cells** covering NYC (r8 ≈ 0.737 km², i.e. exactly
the area of the epic's r9-plus-k-ring work unit):

```
                    parcels per r8 cell
  cells                          1,192
  min                                1
  median                           638
  mean                             719
  p90                            1,587
  p99                            2,103
  max                            2,422
```

Footprints run **1.0–1.6×** the parcel count in dense cells. Worked examples:

```
  882a107707fffff   2,422 parcels   2,657 footprints   =  5,079 features
  882a107631fffff   2,385 parcels   3,499 footprints   =  5,884 features
  882a107733fffff   2,367 parcels   3,644 footprints   =  6,011 features
```

**Median work unit ≈ 638 parcels + ~700 footprints ≈ 1,340 features. Worst ≈ 6,000.**

## C.3 The diagnosis: the sizing was computed for the blocking cell, not the work unit

A single r9 cell is 0.105 km², one seventh of an r8. Median parcels per r9 cell is therefore
≈ 638/7 ≈ 91, plus ~100 footprints ≈ **190 features — which is exactly the plan's "~200."**

> **The "~200 features" figure is correct for ONE r9 cell and wrong by 7× for the r9 +
> k-ring 1 work unit the epic actually specifies.**

The earlier 193-feature observation (Appendix B) came from a 150 m radius disc = 0.071 km²,
about one tenth of an r8 — consistent with, and therefore not contradicting, this result. It
simply measured a much smaller area than the stated work unit.

## C.4 Why this breaks the commercial argument

```
  n =   200    n³ = 8.0e6      free
  n = 1,340    n³ = 2.4e9      seconds, not milliseconds
  n = 6,000    n³ = 2.2e11     minutes per tile
```

The thesis is *"we spend 500× what a spatial join costs, and that is the entire commercial
thesis."* At the measured median that becomes several thousand times, and the ~0.5 s/tile
budget and the ~140 CPU-hour national pass in §13 are **not supported**.

## C.5 Options, none of them free

1. **Work unit = one r9 cell.** Restores ~190 features and the whole cost model, but
   sacrifices the k-ring halo that exists so boundary features resolve. Boundary handling
   would need a different mechanism.
2. **Accept 7× and re-price.** Honest, and it makes the national pass materially more
   expensive; must be re-estimated rather than asserted.
3. **Drop to r10 + k-ring.** r10 is 7× smaller again, so the work unit returns to ~190
   features — at the cost of 7× more tiles and more boundary surface per unit area.
4. **Decompose within the tile.** Only viable if a filter actually decomposes dense tiles,
   which Appendix B shows centroid proximity does not. Depends entirely on bd-3un6.

**Option 3 is the most likely answer** — it preserves the halo argument and restores the
sizing — but it must be measured, not assumed, because boundary-crossing features scale with
perimeter and r10 has considerably more perimeter per unit area.

## C.6 A second live format defect

`H3_R8` is stored as **INTEGER** in `NYC_DCP_MAPPLUTO_HOT` (e.g. `613229552600088575`) and
as **TEXT** in `NYC_BUILDING_FOOTPRINTS_HOT` (e.g. `'882a107707fffff'`).

This was not deduced — it was observed failing. A first query comparing the two directly
returned `FOOTPRINT_COUNT: 0` for every cell, which reads as *"there are no buildings in
this cell"* rather than raising a type error. **A naive join between the two NYC tables on
H3_R8 silently returns nothing.**

That is now the second such defect in the same table pair, alongside `BBL` being
`"1014477501.0"` in MapPLUTO and `"1014470001"` in the footprints table. **Both tables are
NYC municipal sources landed by the same pipeline into the same schema.** Cross-source
normalization is a first-class ingest concern, not a tidy-up.

## C.7 Scope

Measured across **all 1,192 NYC r8 cells**, so unlike Appendix B this is not n=1. It is
still NYC-only, and NYC is the dense extreme — a national distribution will have a far
longer low-density tail. **The sizing must be re-measured per geography before any national
cost estimate is quoted.**

## C.8 Fresh pinned reproduction and a reach denominator

The 2026-08-28 pinned rerun reproduced the parcel distribution exactly: 1,192
parcel-containing r8 home cells, 856,614 distinct BBLs, median 637.5, p99 2,103.27, and
max 2,422. With active NYC footprints pinned to `2026-08-09`, those cells contain
1,081,175 of 1,081,999 distinct active footprints. The remaining **824 footprints have a
valid H3 home cell, but that cell has no parcel-centroid home**. They are not solver
residuals; they are an upstream candidate-reach population.

The fresh two-source total-feature median is 1,395.5, p99 4,824.17, and max 6,011. This
does not restore the tile-wide cost model; it further supports component-wise solving.
Snowflake query `01c6b1d2-0821-784b-006c-c7030888c4da`; executable query:
`scripts/geo_measurements/appendix_c_r8_density.sql`.

---

# Appendix D — MEASURED: the predicate is load-bearing, and the obvious one fails

Added 2026-08-15. **The first measurement that supports the architecture rather than
falsifying it — conditional on a predicate choice that is not the obvious one.**

> **CURRENT STATUS — PREDICATE RETAINED WITH DOMAIN AND CANDIDATE-REACH CONDITIONS;
> DENOMINATOR CORRECTED.**
> Appendix F replaces the asserted `SHAPE_AREA` denominator used for the original 84%/16%
> result with computed geometric area. Appendix D.9 then separates the legacy same-H3-home
> candidate restriction from a bbox-complete reference over the pinned parcel snapshot. In
> dense Brooklyn, same-home-cell lookup reports 22 no-majority footprints; complete reach
> leaves only 2. In the Bronx, the split is 4 versus 1. The greater-than-50% uniqueness proof
> also requires an interior-disjoint parcel domain; it is not valid across overlapping
> condo-unit, billing-lot, and parent-lot geometries. A production controlled halo must
> reproduce the complete-reference result before the remaining residual is called geometric.

> **GEOGRAPHY LABEL CORRECTION — 2026-08-29.** A fresh release-pinned
> borough/coordinate control proved H3 cell `882a100d8bfffff` is Brooklyn, not
> Manhattan: all 2,343 MapPLUTO rows are `BOROUGH='BK'`, all 2,354 footprint
> BBLs use borough prefix 3, and the centroid bounds are longitude
> -73.9361..-73.9236 / latitude 40.6811..40.6897. The original `MN_DENSE`
> label was false. Predicate/reach counts remain numerically valid at that cell,
> but every Manhattan-specific interpretation is superseded by dense Brooklyn.

## D.1 The question

Appendix B showed centroid proximity does not decompose a dense tile at any radius. But
centroid distance was never the intended filter; the architecture says a footprint belongs
to a slot when it is *geometrically compatible* with a parcel. So: does a real polygon
predicate decompose the tile?

## D.2 The measurement

H3 r8 cell `882a100d8bfffff` — **2,343 MapPLUTO lots, 2,354 NYC building footprints.**
Predicates computed server-side in Snowflake; no geometry shipped. Per footprint, how many
parcels does it match?

```
  predicate                          edges    zero        exactly one   more than one
  A  ST_INTERSECTS                   4,718      17 ( 1%)    240 (10%)    2,097 (89%)
  B  ST_CONTAINS                       179   2,175 (92%)    179 ( 8%)        0 ( 0%)
  C  intersects AND >50% of footprint
     geometric area inside parcel    2,332      22 ( 1%)  2,332 (99%)        0 ( 0%)
```

## D.3 What each predicate does

**A — `ST_INTERSECTS` fails outright.** 89% of footprints touch more than one parcel. In a
dense block of contiguous row buildings, a footprint touches the lot lines of its
neighbours, so "intersects" means "is somewhere on this block." Every footprint would chain
several parcels together and **the tile would not decompose at all** — the same failure as
centroid proximity, for a different reason.

**B — `ST_CONTAINS` fails in the opposite direction.** 92% of footprints are contained in
*zero* parcels. Buildings routinely cross lot lines, and the two layers are independently
digitized at different vintages, so strict nesting almost never holds. Only 179 of 2,354
footprints sit entirely inside a lot.

**C — geometric-area majority works empirically on this stratum and has a clean conditional
theorem.** 99% of footprints have exactly one parcel and zero have more than one in this
measured cell. At most one majority holder is guaranteed when candidate parcel interiors
are disjoint, because two disjoint intersections cannot each exceed half of the same
footprint area. NYC's parcel layers are not globally disjoint: condo-unit, billing-lot, and
parent-lot polygons can overlap. Those hierarchies must be typed or stratified before
invoking the theorem. The 22 same-cell zeroes are not all geometric abstentions: D.9 finds
that 20 have a majority parcel in a different H3 home cell. Only the residual after a
candidate-complete controlled halo may be attributed to lot-line straddling or geometry
disagreement.

## D.4 The decomposition result

On the measured interior-disjoint stratum, predicate C gives each footprint at most one
parcel edge, so the compatibility graph is a **forest**. Components are exactly *one parcel
plus the footprints whose area it majority-holds* — typically 2–3 variables, far inside
any compilation budget. The compiler must check or construct that stratum; it must not
infer global forest structure from the threshold alone.

> **Polygon area-majority decomposes the tile. `ST_INTERSECTS` — the predicate a competent
> engineer reaches for first — does not.**

So §6's exact-compilation strategy survives Appendix B's falsification, but *only* with the
right predicate. The choice is not a detail; it is the difference between a forest and a
single connected block.

## D.5 This is ρ working exactly as specified

The three predicates are three readings of the same evidence, and §3's discipline picks the
right one without any tuning:

- `ST_INTERSECTS` is the **unsound** reading — "touches" is not "is on," and admitting it
  asserts more than the geometry supports in the wrong direction.
- `ST_CONTAINS` is **over-strict** — it demands a nesting the two independently-digitized
  layers do not have, so it refuses almost everything.
- **Geometric-area majority is admissible only with declared units, value origin, and
  parcel-domain topology**: it is a weak reading of "this building is on this lot," and it
  fails to a named abstention rather than to a guess.

**The 50% threshold is doing no tuning work inside an interior-disjoint domain** — it is the
boundary above which at most one match is possible. Across overlapping legal parcel
hierarchies it provides no such guarantee. Any higher value trades coverage for a narrower
relation and therefore needs a source contract; it cannot be called automatically sound.

## D.6 The honest caveat

The same objection the adversarial review raised about the `MAPPLUTO_BBL` bridge applies
here in weaker form: **if 99% of footprint-to-parcel assignment is decided by a single
deterministic predicate, the constraint machinery is not being exercised at that level.**
Parcel-to-building assignment is largely a solved geometric join.

That is fine, and it should be stated plainly rather than counted as a win: the architecture
earns its keep at the level *above* — which parcels and buildings constitute the asserted
**property** — not at the level of which building sits on which lot. **The
candidate-complete no-majority residual is where the interesting work is.** Under the fresh
D.9 reference it is 2/2,354 in dense Brooklyn, not the 22/2,354 produced by same-home-cell
blocking. The larger apparent residual was mostly boundary reach, not an assemblage
population.

## D.7 Stratified check — two strata verified, and it improves at lower density

Re-ran predicate C on a second cell at very different density. Both figures below are the
retained 2026-08-15 structured results; D.9's release-pinned rerun supersedes their current
same-cell and candidate-complete counts.

```
  cell               borough   parcels  footprints   exactly one     zero
  882a100d8bfffff    BK/dense    2,343       2,354   2,332 (99%)    22 ( 1%)
  882a100f4dfffff    BX            300         291     291 (100%)     0 ( 0%)
```

**The original run suggested the predicate gets cleaner as density falls.** D.9 narrows
that claim: the current pinned Bronx result is 287/4 under same-cell blocking and 290/1
under complete candidate reach, while dense Brooklyn is 2,332/22 and 2,352/2. Density
may still matter, but H3 boundary reach was confounded with geometry and must be separated.

Two strata across a 7.8× density range both produce a forest. **The decomposition property
is not an artifact of one cell.**

**Scope, honestly.** Three further cells (Queens ~1,500, Queens ~700, Manhattan ~41) were
queried and returned no usable structured output. Loom emitted prose for two of them
claiming a "multi-match rate" of ~3%. That is impossible only in an interior-disjoint
parcel domain; it is possible when legal parcel geometries overlap. **That prose is not
cited and should not be trusted**, but the reason is missing structured output, not a
globally valid uniqueness theorem. Those strata remain unmeasured in this appendix; see
Appendix F for the later structured runs.

## D.8 Consequences

1. **Adopt geometric-area majority for interior-disjoint parcel strata.** Record
   `ST_INTERSECTS` as a rejected candidate with this measurement, and route overlapping
   parcel hierarchies through typed containment/crosswalk constraints.
2. **§6's decomposition claim is restored** for parcel↔footprint, with components of ~2–3
   rather than the estimated 6–20. Appendix C's tile-sizing problem is *unaffected* and
   still stands.
3. **The no-majority population needs its own path** — it is not an error and must not
   be dropped.
4. Still **n=1 cell**, dense Brooklyn. Re-measure across strata per bd-3un6.

## D.9 Fresh pinned rerun: candidate reach precedes predicate truth

The 2026-08-28 rerun pinned MapPLUTO to `26v1` / `2026-05-01` and active NYC footprints
to `2026-08-09`. It measured two candidate universes separately:

- **same H3 home cell** — the legacy Appendix-D restriction on both parcel and footprint
  centroids;
- **complete bbox reference** — every parcel in the pinned snapshot remains eligible
  behind a complete bounding-box prefilter, followed by warehouse `ST_INTERSECTS` and
  computed-area majority.

| Cell | Footprints | same-cell one / zero / multi | complete-reference one / zero / multi | repaired only by cross-home parcel |
|---|---:|---:|---:|---:|
| Bronx `882a100f4dfffff` | 291 | 287 / 4 / 0 | 290 / 1 / 0 | 3 |
| Brooklyn `882a100d8bfffff` | 2,354 | 2,332 / 22 / 0 | 2,352 / 2 / 0 | 20 |

The same-cell A/B/C query also found zero positive-area parcel-overlap pairs within each
home-cell parcel population and reconciled every predicate bucket to its footprint
denominator. The complete reference observed zero majority multi-matches. These are
empirical topology checks on two pinned strata, not a global proof that NYC parcel
hierarchies are disjoint.

The correction is architectural: H3 home-cell equality is ownership metadata, not a
complete spatial candidate predicate. A production work unit remains **tile + controlled
halo**, and its reach must be tested against the complete reference. The complete reference
is an audit oracle, not a proposal to solve all 856,614 parcels monolithically; exact
residual solving still occurs only on the bounded local incidence components.

Executable SQL:
`scripts/geo_measurements/appendix_d_predicates.sql` and
`scripts/geo_measurements/appendix_d_candidate_reach.sql`. Fresh file-exact Snowflake
queries: `01c6b1c0-0821-83a1-006c-c7030888b8de` and
`01c6b1c0-0821-784b-006c-c7030888c3c6`.

## D.10 Source-bound geom-v3 rerun: r8+k1 matches the bounded reference

On 2026-08-29, the candidate-reach audit was rerun with MapPLUTO geometry from
`NYC_DCP_MAPPLUTO_GEOM_V3_EXT`, H3 home cells joined from the same pinned HOT
release, and explicit h3o r8+k1 work cells emitted by Canon. It measured three
candidate planes independently:

- **same cell** — legacy center-cell equality;
- **controlled halo** — the center plus its six r8 neighbors;
- **complete bbox reference** — every release-pinned parcel remains eligible behind
  the bbox prefilter.

| Cell | Footprints | same-cell one / zero | r8+k1 one / zero | complete-reference one / zero / multi | reference truth outside k1 | repaired by k1 |
|---|---:|---:|---:|---:|---:|---:|
| Bronx `882a100f4dfffff` | 291 | 287 / 4 | 290 / 1 | 290 / 1 / 0 | 0 | 3 |
| Brooklyn `882a100d8bfffff` | 2,354 | 2,333 / 21 | 2,353 / 1 | 2,353 / 1 / 0 | 0 | 20 |

All denominators reconciled. The Brooklyn result changes one row relative to the
2026-08-28 HOT-only measurement; this is a real geometry/transform-plane change, so
the old receipt remains historical and the geom-v3 result supersedes it for
source-bound work. Query `01c6b6f9-0821-83a1-006c-c703088a39aa` ran in 10,819 ms.

This is the first positive controlled-halo reach result: k1 reproduced the complete
bounded reference in both measured cells. It is not a global recall proof, a claim
about another resolution or source, or a solver-correctness result. Snowflake
GEOGRAPHY predicates provide the empirical comparison; Canon's exactness remains
relative to quantized local integer geometry. The complete reference is an audit
oracle only, never a proposal to solve the national parcel population together.

## D.11 Stratified geom-v3 rerun: r8/r9 reach and incidence are different scales

On 2026-08-30, the controlled-halo audit expanded to six r8 strata across all five
boroughs and one stress-selected logical r9 child per stratum. Selection ranks the
combined parcel-plus-footprint population whose representative points independently
bin to the r8 stratum; it is deliberately stress-biased, not a random or
population-representative sample. MapPLUTO is pinned to geom-v3 `26v2` /
`2026-08-01`; active NYC footprints remain pinned to `2026-08-09`.

Snowflake's two current point-to-cell functions first reproduced h3o's known r9 answer
`892a100d62bffff` for the historical bad control point. Representative points were then
assigned in Snowflake, while every seven-cell neighbor disk was emitted by
`canon geo tile-work`. This repairs the old helper control enough for a warehouse
measurement; it does **not** prove full-population Snowflake↔h3o parity.

H3's hierarchy has exact logical ancestry but only approximate geometric containment.
Consequently a point in a selected r9 cell may independently bin to an adjacent r8 cell
rather than that r9 cell's logical parent. The selection receipt now reports both grains.
Complete-r9 versus selection-in-r8 populations differed in Manhattan small by 10 parcels
and 10 footprints, Queens medium by 20 and 35, and Staten Island by 0 and 6; the other
three strata had no difference. The r9 table below uses the complete r9 populations.

| Stratum | Work nodes r8 / r9 | Target footprints r8 / r9 | same-cell one/zero r8 | k1/global one/zero r8 | same-cell one/zero r9 | k1/global one/zero r9 | max incidence component r8 / r9 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Brooklyn dense | 25,786 / 4,670 | 2,354 / 375 | 2,333 / 21 | 2,353 / 1 | 366 / 9 | 374 / 1 | 5 / 5 |
| Bronx lower | 2,688 / 617 | 291 / 69 | 287 / 4 | 290 / 1 | 67 / 2 | 69 / 0 | 9 / 3 |
| Manhattan small | 2,403 / 378 | 45 / 34 | 42 / 3 | 45 / 0 | 34 / 0 | 34 / 0 | 4 / 3 |
| Queens dense | 15,062 / 2,451 | 2,007 / 362 | 1,993 / 14 | 2,007 / 0 | 352 / 10 | 362 / 0 | 4 / 4 |
| Queens medium | 11,856 / 3,489 | 1,049 / 386 | 1,036 / 13 | 1,044 / 5 | 372 / 14 | 386 / 0 | 5 / 4 |
| Staten Island low | 2,260 / 662 | 256 / 193 | 204 / 52 | 256 / 0 | 153 / 40 | 193 / 0 | 71 / 65 |

Across the six r8 centers, k1 recovered 100 same-cell misses and matched the complete
reference for all 6,002 target footprints: 5,995 had one majority parcel, 7 had none,
and zero had multiple majority parcels. Across the six r9 centers, k1 recovered 74
same-cell misses and likewise matched the complete reference for all 1,419 targets:
1,418 one, 1 none, 0 multiple. `truth_outside_k1 = 0` in every row. Work-unit,
same/k1/global denominator, reach, forest, and component-accounting sanity checks all
passed.

The component graph contains every parcel in the k1 work unit plus center-owned
footprints, with an edge only for the computed-geometric-area majority predicate.
Therefore its many isolated parcels are real in this graph and its component maximum is
more informative than its median of one. The Staten Island parcel-star persists at both
resolutions (71 at r8, 65 at r9); r9 reduces work volume without removing the long-tail
shape. These components are **not** claimed as final solver widths. Address, deed,
collateral-set, attribute, and temporal constraints can join parcel variables that this
single geometric channel leaves separate.

The deterministic r9-center selection query is
`scripts/geo_measurements/appendix_d_stratified_halo_centers.sql`; file-exact Snowflake
query `01c6bc81-0821-9afc-006c-c703088c04f6` ran in 7,171 ms. It returns both the
selection-in-r8 and complete-r9 denominators for all six declared centers, with both
population-partition sanity checks passing. The main executable query is
`scripts/geo_measurements/appendix_d_stratified_halo.sql`; file-exact Snowflake query
`01c6bc7d-0821-a0dc-006c-c703088c231a` ran in 23,572 ms and returned twelve rows. It
contains two sources only; FEMA, other footprint layers, client coverage holes, exact
local-integer replay, citywide recall, and solver compilation/runtime distributions
remain open.

---

# Appendix E — MEASURED: both kill-criterion baselines, tier-resolved

Added 2026-08-16 (bd-14co; full tables and exact SQL in
`docs/geo_design_session/BASELINES_BD14CO.md`). **The two numbers the cascade must beat
now exist, and the tier breakdown localizes exactly where the product's work lives.**
All figures from returned structured results against MapPLUTO release **26v1**
(2026-05-01), five-borough CMBS geocode scope, fan-out-aware distinct-point grain.

## E.1 The two baseline points

```
  naive address-string   28.89% coverage (1,522/5,269 address-county keys), zero multi-match,
                         97.70% house-number agreement on the keys where it fires
  geometry-only PIP      94.65% coverage (3,858/4,076 distinct points), zero multi-lot points
                         on 26v1, 71.41% house-number agreement on comparable hits
```

The cascade must pass **above both points on the coverage/precision plane**: materially
more coverage than 28.89% at address-grade precision, and higher precision than
geometry-only near 94.65% coverage. Precision for both baselines remains unmeasured —
scoring against PLUTO address→BBL is circular (the CMBS address is the thing under test);
it is blocked on bd-179b's address-independent ACRIS ground truth.

**Snapshot correction:** the previously recorded 157 multi-lot points do not reproduce on
26v1 — both `ST_CONTAINS` and `ST_INTERSECTS` now return only one-lot hit points. The
condo parent/unit overlap population is a property of a specific MapPLUTO release, not a
stable fact. Do not quote it without pinning its snapshot.

## E.2 The silent-error tier, quantified

`nearest_rooftop_match` (344 points, 8.4% of the tile-scope corpus) is where both
channels fail **on the same population**:

```
  tier                    PIP hit   house-number agree   chimera   address-match fires
  rooftop                  99.91%        78.00%            5.98%        34.13%
  nearest_rooftop_match   100.00%        48.40%           14.53%         1.52%
  range_interpolation      53.02%        13.94%            5.94%        24.12%
```

Geometry is maximally confident exactly where it is least trustworthy (100% hit, less
than half house-number agreement, 2.4× the rooftop chimera rate), and naive address
matching almost never fires there (9/593 keys). A cascade that arbitrates between the
two channels has nothing to arbitrate *with* on this tier — it needs the constraint
frame's independent evidence (footprints, address ranges, parity, temporal). **This is
the population the architecture exists for, and it is now a named, measurable slice.**

## E.3 Fan-out at this grain

95.53% of surrogate property keys hit exactly one lot (6,033/6,315); 0.05% hit two;
4.42% hit zero. The single-point, single-lot case dominates the geocode grain — the
assemblage/multi-parcel problem lives almost entirely below the surface of this table
and must come from documents (bd-179b, bd-1oid), not from geocode fan-out.

---

# Appendix F — MEASURED: decomposition survives stratification and a second footprint source; Appendix D's 16% was a denominator artifact

Added 2026-08-16 (bd-3un6; full tables and exact SQL in
`docs/geo_design_session/STRATA_FEMA_BD3UN6.md`). Six NYC cells now measured across a
~57× density range, plus the first genuine multi-source test using the newly landed
`FEMA_USA_STRUCTURES_HOT` (135.3M rows, both TEXT and INT H3 keys — the C.6 defect class
now has typed companions).

## F.1 Stratified legacy predicate: observed forests, with one new shape

Four new strata (MN 41 parcels, QN 1,502, QN 701, SI 101), all with **zero
multi-matches** under the Appendix-D-compatible `SHAPE_AREA` denominator. These are
observations, not an unconditional uniqueness theorem; the table mixed geometric
intersection areas with a source-asserted denominator and therefore is not the canonical
predicate now specified in F.2:

```
  cell        borough  parcels  footprints  exactly-one    zero      max component
  882a1008c7  MN            41          45   41 (91.1%)   4 ( 8.9%)      4
  882a103b6b  QN         1,502       2,007   1,753 (87.3%) 254 (12.7%)   4
  882a100e25  QN           701       1,049   927 (88.4%)  122 (11.6%)    5
  882a106019  SI           101         256   154 (60.2%)  102 (39.8%)   71
```

Three cells decompose into components of ≤5. The Staten Island cell is the new shape:
still a forest, but with **parcel-star components of 39 and 71** — single large parcels
holding many structures (campus/complex fabric). Exact compilation survives, but the
per-component budget must be sized by the largest parcel-star, not by a universal
"2–3 variables."

## F.2 The denominator correction to Appendix D

Appendix D's 84%/16% dense-Brooklyn split is reproduced **only** with the source-asserted
`SHAPE_AREA` as the fraction's denominator. The pure geometric predicate
`ST_AREA(intersection)/ST_AREA(footprint)` on the same cell gives **2,332 / 22 / 0**
(99.1% exactly-one, 0.9% no-majority). Of the 366 "no-majority" footprints, 344 resolve
under the literal geometric denominator, 311 are clean two-parcel straddlers by
intersection count, and 300 have ≥99% of their geometric area inside their top two
parcels.

**The 16% population was mostly an artifact of dividing a computed area by an asserted
one.** The same failure appeared independently in FEMA's `SQMETERS` field (2 multi-matches
in Queens where geometry gives 0) and in a units-conversion probe (946 impossible
multi-matches — dimensional error, caught by the multi=0 sanity gate). This is ρ working
as specified, one level down: **asserted source area fields are observations to check,
never denominators to divide by.** Adopt `ST_AREA`-over-`ST_AREA` as the canonical
predicate-C form. Under legacy same-cell blocking the no-majority rate is ~1%, not 16%;
D.9's complete-reference audit reduces it further to 2/2,354.

## F.3 The multi-source result is retained but not canonical

The recorded three-layer merged graph (parcels + NYC footprints + FEMA structures) used a
geometric denominator for FEMA but the superseded source `SHAPE_AREA` denominator for NYC.
Its literal retained output is:

```
  cell           NYC exact/zero   FEMA exact/zero   merged components  merged max
  BX  882a100f4d   274 / 17         76 /  39            356               19
  BK  882a100d8b  1,988 / 366       88 / 152          2,861                6
  QN  882a103b6b  1,753 / 254     1,078 /  30         1,786                6
```

**That mixed-contract merged graph was a forest in all three cells.** It does not establish
that the canonical geometric-over-geometric multi-source graph remains a forest; that
requires a rerun from the preserved SQL with both channels corrected and overlapping legal
parcel domains typed. FEMA
coverage is strongly geography-dependent: 97.3% majority-parcel rate in Queens vs 36.7%
in dense Brooklyn (FEMA sees only 240 structures where NYC sees 2,354) — FEMA is a
corroborating source in outer-borough fabric and nearly absent in the urban core.

## F.4 Cross-source agreement is real but asymmetric

On the Queens cell, NYC and FEMA agree on per-parcel structure counts for 55.2% of
parcels (829/1,502); where they disagree, NYC sees more on 634 parcels vs FEMA's 39, and
no disagreement exceeds 3. Over-segmentation vs `NUMBLDGS` is negligible (max overage 1).
The dominant disagreement mode is FEMA missing whole runs of 2-structure residential
parcels, not over-segmentation — so Régin-style within-source exclusivity has little to
catch at the footprint level in this fabric, and the `gcc` coverage-rate constraint
(§3's FEMA row) is the right admission form.

## F.5 Consequences

1. **The retained runs show promising decomposition at n=6 cells, but do not yet affirm the
   canonical multi-source strategy.** The corrected denominator and overlapping-parcel
   domain must be rerun together. Appendix G separately settles the r10+k-ring sizing
   arithmetic.
2. **Appendix D's predicate is right; its denominator was wrong.** Canonical form is
   geometric-over-geometric. The "16% product population" shrinks first to a ~1%
   same-cell residual and then to 2/2,354 under D.9's complete-reference reach. Only a
   controlled-halo rerun can name the production hard residual; parcel-star components
   remain a separate measured stress shape.
3. **Parcel-star components (retained SI max 71) are a measured stress shape** for
   per-component compilation budgets, not a proven global bound.
4. All measurements NYC-only; the bead's suburban/agency-multifamily stratum has no
   landed source yet.

## F.6 Fresh Overture third-plane rerun: positive reach, correlated lineage, wider raw stars

On 2026-08-30, `scripts/geo_measurements/appendix_f_overture_three_source.sql`
added release-pinned Overture buildings (`2026-07-22.0` / `2026-07-22`) to the
same six r8 and six r9 strata used by D.11. MapPLUTO remained pinned to geom-v3
`26v2` / `2026-08-01`; NYC footprints remained pinned to `2026-08-09`. The
query returned 24 nonzero source-stratum rows in 35,772 ms under Snowflake query
`01c6bcc3-0821-a0dc-006c-c703088c2682`. Every work-unit, denominator, reach,
source-forest, source-count, and component-accounting check passed.

| Resolution | Source | Center observations | same-cell one / zero / multi | k1 = complete one / zero / multi | OSM lineage |
|---|---|---:|---:|---:|---:|
| r8 | NYC footprints | 6,002 | 5,895 / 107 / 0 | 5,995 / 7 / 0 | n/a |
| r8 | Overture buildings | 6,018 | 5,917 / 101 / 0 | 6,005 / 13 / 0 | 5,967 |
| r9 | NYC footprints | 1,419 | 1,344 / 75 / 0 | 1,418 / 1 / 0 | n/a |
| r9 | Overture buildings | 1,401 | 1,334 / 67 / 0 | 1,400 / 1 / 0 | 1,393 |

`truth_outside_k1=0` for every source-stratum row. This is a second positive
footprint-plane reach result over exactly the declared strata, not citywide
recall. The complete parcel snapshot remains an audit oracle only; each solve
still receives one bounded center-plus-k1 work unit.

The combined raw-observation work units span 3,709–38,667 nodes at r8 and
596–7,015 at r9. Parcel-star maxima rise from D.11's 4–71 to 7–128 at r8 and
from 3–65 to 5–118 at r9 because geometrically similar observations from two
footprint planes are both present. That is a useful upper bound on the
predicate-incidence graph and a warning against budgeting the solver by source
row count. It is **not** a latent-building count: source-to-source equivalence,
segmentation disagreement, and admission into the actual solver incidence graph
remain unimplemented and unmeasured.

Lineage prevents an even more serious interpretation error. Across the measured
Overture centers, 7,360/7,419 observations (99.20%) declare OpenStreetMap in
`SOURCES_JSON`. A direct OSM/Overpass building polygon is therefore usually the
same upstream evidence, not an independent corroborating vote. Direct OSM can
still add a useful *semantic* plane—entrances, address tags, building parts,
names, uses, levels, and POIs—but a production artifact must pin extract bytes or
a replication sequence, preserve ODbL attribution, and retain OSM record/version
lineage. A mutable live Overpass response is suitable for a bounded capability
probe, not a reproducible solver input.

The upstream landing is only partially repaired. The base table exposes
6,443,512 distinct New York building features with valid H3 anchors (query
`01c6bcbd-0821-a0dc-006c-c703088c24fe`, 1,315 ms), so the bounded measurement is
real. However, `OVERTURE_MAPS_FEATURE_H3_COVERAGE` still returns zero pinned
building rows (`01c6bcbd-0821-a0dc-006c-c703088c2502`), and
`OVERTURE_MAPS_BUILDINGS_HOT` fails compilation because its declared 28 columns
do not match the 33 produced by its view query
(`01c6bcbc-0821-9afc-006c-c703088c06e6`). The landing bead remains open until
those contracts are repaired; bypassing them through the working base table is
measurement progress, not closure.

---

# Appendix G — MEASURED: no k-ring configuration restores "~200 features"; the cost model must be component-wise

Added 2026-08-16 (bd-152l; full tables and exact SQL in
`docs/geo_design_session/WORKUNIT_SIZING_BD152L.md`). This settles Appendix C's Option 3
with data: measured across every parcel-containing NYC work unit, three sources
(parcels + NYC footprints + FEMA structures), centroid-derived r9/r10 home cells (no
landed table has native r9/r10 columns).

## G.1 The distributions

```
  work unit   centers   median   mean     p90     p99     max    >200      >400
  r9  + k1      6,829    2,274   2,442   4,619   6,103   7,515   94.77%    90.76%
  r10 + k1     39,098      418     421     755   1,011   1,329   75.88%    52.14%
```

r10+k1 cuts the median 5.44× — **and still does not restore ~190.** Three-quarters of
r10 work units exceed 200 features; half exceed 400. The "~200 features, so O(n³) is
free" arithmetic does not hold at any measured k-ring configuration once FEMA is
included and the halo is retained.

## G.2 The boundary tax, measured

Fraction of features whose geometry is not contained in their centroid's home cell:

```
  source           r9        r10
  parcels        15.41%    37.24%
  NYC footprints  8.07%    20.33%
  FEMA           10.86%    26.14%
```

Moving r9→r10 raises boundary pressure ~2.4–2.6× across all three sources — Appendix
C's perimeter warning, quantified. At r10 more than a third of parcels straddle their
home cell. (Direct `H3_COVERAGE_STRINGS` aggregation timed out twice; the containment
predicate above is the recorded geometric substitute.)

## G.3 Verdict on Appendix C's options

**Option 3 is directionally right and numerically insufficient.** No uniform-grid work
unit both preserves the halo and lands near 200 features in NYC. The honest reading,
combining this with Appendix F:

1. **The tile-wide O(n³) framing is the wrong cost model.** Appendix F shows the
   compatibility graph decomposes into parcel-star components (max 71) under the
   canonical geometric predicate — so per-work-unit cost is driven by component-wise
   compilation plus halo reconciliation, not by tile-wide consistency passes over n.
2. **§13's national-pass estimate must be re-derived** from: 39,098 r10 units × NYC-scale
   component distributions, or priced at r9 with in-tile decomposition. Until then, quote
   neither 0.5 s/tile nor 140 CPU-hours.
3. **Boundary reconciliation is a first-class cost** at r10 (20–37% of features), not an
   edge case; bd-2b9d's halo design should assume it.
4. NYC-only, land-biased center universe (parcel-containing cells), all three sources —
   denominators differ from Appendix C's all-1,192-r8-cell, two-source measure by
   construction; both are recorded with their SQL.

---

# Appendix H — MEASURED: the ACRIS truth set exists, caught its own contamination, and puts provisional precision far below coverage

Added 2026-08-16 (bd-179b; full tables and exact SQL in
`docs/geo_design_session/GROUNDTRUTH_ACRIS_BD179B.md`). First attempt at address-independent
ground truth for the Appendix E baselines: CMBS loans matched to recorded ACRIS mortgages by
**amount + recording-date window without an address-string match** (bridge:
`PROPERTY_PERIOD_FACT → LOAN_ISSUANCE` on CIK+ASSETNUMBER; 3,040 five-borough loans).
The original run did, however, scope borough through a field derived from the geocoder's
`COUNTY_FIPS`. H.7 records the later provenance finding and the controlling rebuild from
raw filed `PROPERTYCOUNTY`; the original run remains a historical diagnostic, not the
release truth plane.

## H.1 The gate, and what it accepted

Operating point exact-cents ± 30 days, unique-or-discard: **523 accepts** (1,230
ambiguous discarded, 1,287 no-match), 392 → one BBL, 131 → 2+ BBLs. Truth coverage of
the baseline grains is honest and low: 582/4,076 points (14.28%), 864/5,269 address keys
(16.40%).

## H.2 The raw headline, and why it must not be quoted bare

Scored any-overlap (lenient): geometry PIP **29.48%** (166/563), naive address **23.43%**
(67/286) on truth-covered units. Condo unit-lot representation explains only ~5pp
(block-grade bounds 34.28% / 28.67%). The dominant failure signature was full-block
mismatch — which is also the signature of a *wrong unique match*. The contamination
probe settled it with three independent discriminators:

```
  signal                     lot-correct accepts        full-block mismatches
  recording offset           0 negative, median +12d    165/356 negative, spans window
  ACRIS legal borough        agrees 135/135             disagrees 203/356 (113 w/ county too)
  amount roundness           non-round: 55.46% precise  $1M multiples: 7.88% precise
```

Real recordings happen days-to-weeks *after* origination in the property's borough;
collisions are uniform in time, cross-borough, and concentrated in round amounts.
**Amount+date uniqueness alone is not a sufficient truth gate.** The raw 29.48% is a
contaminated estimate and the report says so explicitly.

## H.3 What survives as the provisional precision point

On the cleanest measurable slice (non-round amounts, 119 loans): **geometry PIP ≈ 55%
(66/119)** against document truth — with ~95% coverage. The coverage/precision plane the
cascade must beat is therefore provisionally: address-string (28.89% coverage, high
precision when it fires), geometry (≈95% coverage, ≈55% precision on clean truth). The
gap between them is the product. Refine the gate (non-negative offset, legal-borough
agreement, roundness handling or a second discriminator such as lender-name tokens) and
re-score before treating any precision number as final.

## H.4 The assemblage payoff

Of 125 multi-BBL loans invisible to the geocode grain, condo-signature filtering leaves
**79 genuine multi-parcel candidates** (24 spanning multiple blocks). That is the first
measured count of the invisible-assemblage population — the exact population §2's
architecture exists to resolve — subject to the same gate-refinement caveat.

## H.5 The meta-result

The truth set audited itself: declared bands (unique-or-discard) plus independent
consistency checks (offset sign, borough, roundness) converted "suspiciously low
precision" into a *named, attributable defect of the truth gate* rather than a silently
wrong conclusion. This is §3.2's band-versus-threshold argument operating one level up —
the strongest process evidence yet that the architecture's self-auditing claim is real.

## H.6 Gate V2 — historical diagnostic, superseded for truth admission by H.7

Same session, completing H.3's mandate. Operating gate: exact cents, recording offset
**[0,+45] days**, ACRIS legal borough must agree with a property county, all 100k/1M-round
amounts dropped, unique-or-discard applied *after* the filters (candidacy recomputed from
scratch). Sensitivity reconciles to 3,040 loans at every window; [0,+45] accepts 166
(48 ambiguous, 451 no-match, 2,375 round-excluded).

```
                       coverage                precision (lot)   precision (block)
  geometry PIP         94.65% of points        154/233 = 66.09%   169/233 = 72.53%
  naive address        28.89% of keys          63/93  = 67.74%    71/93  = 76.34%
  nearest_rooftop PIP  100% of tier            15/29  = 51.72%    18/29  = 62.07%
  nearest_rooftop addr fires 0/44 truth keys   —                  —
```

**Both baselines sit at ~two-thirds precision against document truth.** The plan's
implicit assumption that address matching is high-precision-when-it-fires is also dead:
67.74% at lot grade. The cascade's target is now concrete — materially exceed ~66–68%
precision at geometry's coverage, and the ~34% of geometry answers that are wrong (worst
on nearest_rooftop, the tier where address evidence never fires) are the addressable
population. G6 on v2 accepts: 25 invisible multi-BBL loans, 15 non-condo.

Caveats that travel with these numbers: v2 truth coverage is small (242/4,076 points,
5.94%) and the round-amount exclusion biases the truth set toward odd-amount loans;
lender-name second-discriminator admission of round amounts is the recorded path to a
larger truth set (originator and party fields are landed). The 2026-08-28 provenance audit
also showed that `LOAN_ISSUANCE_PROPERTY.RECORDED_BOROUGH` was derived from geocoded
`COUNTY_FIPS`. Therefore the claim that this plane used no address-derived channel was too
strong. Retain its measurements for sensitivity only; H.7 controls truth admission.

## H.7 Filed-county lender/party rebuild — controlling ACRIS measurement

Measured 2026-08-28 through the repaired live MCP path, after catalog discovery and table
description. The retained measurement used bridge build
`3aed6660-ce1c-46a9-aeb2-7296c134ce8f`; ACRIS is pinned to `RELEASE_DT = 2026-08-10`;
MapPLUTO is scored separately at `26v1 / 2026-05-01 / shoreline_clipped` and
`26v2 / 2026-08-01 / shoreline_clipped`. The truth gate maps the latest raw filed
`PROPERTY_PERIOD_FACT.PROPERTYCOUNTY` to an ACRIS borough and requires raw filed
`PROPERTYSTATE = 'NY'` from the same source. Missing or unrecognized filed counties
abstain, and same-named counties outside New York do not enter H.7. Geocoder-derived
county is not admitted to truth selection.

The bridge build identifies the source snapshot used by that measurement; it is not a
permanent Canon runtime dependency and does not require retention of the entire historical
loan universe. A forward live run explicitly selects an available build and immediately
materializes the bounded accepted cohort, source rows, release pins, selection rule, and
denominators into a content-bound artifact. Replaying a retained measurement uses that
bounded artifact. Selecting a newer build creates a new cohort and cannot be described as
a reproduction of the older one without key-for-key retained evidence.

Fresh control `01c6bd17-0821-a0dc-006c-c703088c2796` (197 ms, 7 rows) reproduced
the 2,974-loan filed-state/county universe exactly: 653 non-round plus 2,321
round. The raw county-only 3,016 result and the geocoder `COUNTY_FIPS` 647/2,291
result are diagnostic-only. The 42 raw county-only extras were same-named counties
outside New York: GA 29, CA 6, VA 5, NC 3, null 3, and NA 1, with overlapping
loans across state buckets.

H.7 truth is therefore scoped as the `nyc_filed_collateral_slice`. A mixed-state
loan admitted through raw New York filed collateral is evaluated only for its NYC
ACRIS slice; the materializer must not silently label that subset as full
national collateral truth.

The declared 2,974-loan universe separates two truth planes. The controlling
fresh 2026-08-30 staging-table rerun is:

```
  plane                                  eligible  unique accepts  reach of plane
  non-round amount/date/legal borough        653             172      26.34%
  round + exact lender/party name           2,321             270      11.63%
  disjoint accepted-loan reach              2,974             442      14.86%
```

The round plane first requires exact equality between the CMBS originator and the ACRIS
lender party after the same deliberately narrow transform: uppercase, replace each
non-alphanumeric character with a space, trim. It does not collapse internal whitespace,
strip legal suffixes, perform token containment, or use fuzzy matching. Lender party type
is document-type-specific: type 2 for `CMTG`, `M&CON`, `MTGE`, `SMTG`, and `SPRD`; type 1
for `MMTG`. Since ACRIS `DOCUMENT_AMT` is floating-point, amount equality is exact only
relative to the declared cents quantization
`ROUND(value * 100, 0)::NUMBER(38,0)`, not exact relative to the source instrument or the
world.

Fresh originator availability control `01c6bd19-0821-9afc-006c-c703088c0936`
(313 ms, 2 rows), with lineage receipts `1385b1fd64bf266f` and
`dbd7d7dbc84727b2`, reports 653/653 non-round and 2,317/2,321 round originator
text availability. That conflicts with the archived G7 availability figures
(605/653 and 2,173/2,321). The release-pinned staging rerun below reproduces
2,317/2,321 and resolves the current-snapshot denominator in favor of the fresh
bridge; the archived figures remain historical evidence.

Fresh round candidate aggregation `01c6bd25-0821-a0dc-006c-c703088c27be`
(42,031 ms, nonzero array row) likewise conflicts with archived G7: it found
2,317 round loans with exact originator text, 311 candidate loans, and 439
loan-document pairs, versus archived 2,173 / 182 / 277. The cached identical
repeat `01c6bd26-0821-9afc-006c-c703088c095a` is not an independent receipt.
The aggregate-to-flatten legal continuation
`01c6bd28-0821-a0dc-006c-c703088c27c6` hit deterministic client cancellation
000604/57014 at 45,044 ms. That raw-table attempt remains discarded; the
staging-table rerun below now supplies fresh legal counts.

Candidate reach and scored precision are different quantities. Of the non-round plane,
262/653 reached an amount/date/legal-borough candidate; 221 had legal confirmation, 172
were uniquely admitted, 49 remained ambiguous, 41 candidate loans had no legal
confirmation, and 391 had no candidate. In the fresh round plane, 311/2,321 reached an
exact lender candidate; 306 had legal confirmation, 270 were unique accepts, 36 remained
ambiguous, five candidate loans had no legal confirmation, and 2,010 had no candidate.
Source count is not treated as independent information, and the two planes are not pooled
into a precision headline.

The retained H.7 measurements define the `retained_complete` multi-BBL replay
contract: 35 non-round accepted loan subjects plus 14 round accepted loan
subjects, selected by accepted ACRIS `BBL_COUNT > 1` after legal acceptance. A
typed artifact may claim `retained_complete` only when the supplied rows carry
those 49 subjects, both pinned MapPLUTO candidate releases per subject (98
release-run rows), matching payload row counts, preserved syntactically
validated source hashes, and at least one non-fixture SQL-bound cited query receipt. Canon
validates the source-hash syntax and identities but does not recompute source
bytes in this offline materializer. The checked in-repo fixture remains
`fixture_subset` with 1+1 subjects. The association-plane split below is a
separate property-key stratum; neither the 49 retained subjects nor the 98
release-run rows satisfy the frozen E4 target of 79 genuine cases.

The 2026-08-30 denominator control found 35 non-round plus 36 round uniquely
accepted multi-BBL subjects, or 71. That count is an observation about its declared
snapshot, not the definition of `live_complete`. `live_complete` means the supplied
nonempty cohort completely reconciles its own build-bound plane denominators and contains
exactly one row for every selected subject and pinned candidate release, with live receipts
and immutable supporting records. The accepted-truth handoff materialized the observed 71
with row-level source records and hashes, but that historical execution becomes a
`live_complete` artifact only after candidate reach and both MapPLUTO releases are added.
Fourteen of the 15 existing Gate V2 loans are inside those
71; one is outside, and the two H.4 extension cases carry no H.7 loan key.
Their maximum currently demonstrated union is therefore 74, not 88. The
frozen 79-case gate remains short by five genuine, non-duplicate cases; it must
not be passed by counting release rows, duplicate truth planes, or
retained/live replays twice.

A fresh 2026-09-01 control against available bridge build
`ce3953ac-c2d4-4b48-bf02-29f0cf341389` read 51,778 bridge rows and again selected
35 non-round plus 36 round multi-BBL subjects. Its plane denominators were independently
reconciled: non-round 652 eligible / 262 candidate / 221 legal-confirmed / 172 accepted /
49 ambiguous / 41 candidate-without-legal / 390 no-candidate; round 2,323 / 312 / 307 /
271 / 36 / 5 / 2,011. The MCP success envelope still omitted a Snowflake query id, so this
is a fresh snapshot-relative denominator control, not a durable source-record export or a
completed `live_complete` artifact. The unavailable historical bridge build is therefore
not a forward blocker; immediate bounded materialization and retrievable execution identity
remain the missing handoff.

The parameterized checked-in `h7_staging_truth_export.sql` then returned 71 non-guard
`h7_staging_accepted_truth_row.v0` rows for that same current build: the first and last rows
carried the declared build id, the separate 35/36 plane denominators, and
`whole_export_rows = 71`. This proves the current build can produce the bounded row-level
accepted-truth cohort without historical warehouse retention. The MCP envelope again
returned `query_id = null`; the export does not yet include the two MapPLUTO candidate
releases or the final immutable source-record byte wrappers, so it is not presented as a
typed `live_complete` population or E4 proof.

The following point-grain PIP table remains the archived 35+14 scored cohort;
the 22 newly admitted round multi-BBL subjects have not been scored and are not
silently pooled into it. Point-grain PIP scoring against document BBL sets,
using the latest geocode observation per
exact point and the same filed-borough association, is:

```
  truth plane / association       truth points  PIP reached  lot correct  block correct
  non-round / single-property              104          100   59 (59.00%)   67 (67.00%)
  non-round / multi-property               153          148  127 (85.81%)  133 (89.86%)
  round exact-lender / single               94           93   69 (74.19%)   79 (84.95%)
  round exact-lender / multi                 99           93   71 (76.34%)   82 (88.17%)
```

The single/multi split is load-bearing. A multi-property loan carries a loan-level ACRIS
BBL set; copying that set to every collateral property is lenient set-valued scoring and
does not establish the exact loan-to-property association. Pooling it with single-property
precision would hide that ambiguity. The sharp non-round gap (59.00% versus 85.81%) is
empirical evidence of the confound. The exact-lender plane is steadier, but it mostly
expands coverage and is not independent adjudication: 146/149 accepted loans are
Manhattan-only and three are Queens-only.

Do not use the G7.7 property-key association strata as the H.7 E4 selector:
the 57 non-round and 51 round multi-property class loans are not the 35/14
accepted ACRIS multi-BBL truth population.

The same score at property-key grain is 67/109 and 131/153 lot-correct for non-round
single/multi, and 72/96 and 78/104 for round exact-lender single/multi. Both MapPLUTO
releases produced the same metrics on all 57 comparable scored strata. That is scoped
equality for this measurement, not evidence of global release equivalence.

The computational shape was itself a scaling test: a monolithic warehouse join hit the
repeatable client-cancellation path, while materializing the small exact-candidate relation
into one array row and binding its few hundred residual pairs as an explicit `VALUES`
relation let the legal confirmation complete. This is the intended mathematics: bounded
candidate section, then a small exact residual—not a national or 500k-candidate
monolithic solve.

A fresh 2026-08-30 staged execution confirms that the upstream repair changed the
physical boundary. Stage 1 non-round query
`01c6befd-0821-9afc-006c-c703088ce26e` completed in 6.966 seconds with guard
`ok` and exported all 653 loan parameters. Corrected Stage 2 shard 0/16 over
`DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER` completed as
`01c6bfc9-0821-a0dc-006c-c703088d15fe` in 9.456 seconds: 40 shard loans,
19 candidate loans, 42 candidate documents, 21 no-candidate loans, and both
denominator checks true. Corrected Stage 3
`01c6bfca-0821-a0dc-006c-c703088d160a` completed in 7.156 seconds against
filed-borough/LEGAL equality with no refused rows.

Round Stage 1 `01c6bfcb-0821-a6c8-006c-c703088d253a` completed in 6.499
seconds and exported all 2,321 parameters. A discarded first rewrite
(`01c6bfcc-0821-a6c8-006c-c703088d2542`) pre-aggregated all 46.5 million
PARTY rows and timed out; it is not evidence. The corrected selective join
collapses duplicate exact-party assertions deterministically after the
candidate restriction. Round shard 0/64 completed as
`01c6bfcd-0821-a6c8-006c-c703088d2546` in 9.499 seconds with 34 loans,
6 candidate loans, 9 candidate documents, 28 no-candidate loans, and both
denominator checks true. Its LEGALS continuation
`01c6bfce-0821-a6c8-006c-c703088d254e` completed in 6.553 seconds with no
refused rows. It includes a live example where MASTER recorded borough 1 is
only diagnostic while filed boroughs 2 and 3 control the LEGALS probes—the
case the superseded ordering would have silently removed.

The file-backed full-plane control
`h7_staging_denominator_control.sql`, query
`01c6bfd2-0821-a0dc-006c-c703088d1612`, completed in 3.508 seconds. It
reproduced the non-round 653/262/221/172/49/41/391 algebra and measured the
fresh round 2,321/311/306/270/36/5/2,010 algebra. All six plane-specific
denominator checks passed. Accepted multi-BBL truth is 35 non-round plus 36
round. This aggregate proves the population counts and subject keys, not the
row-level `live_complete` artifact, candidate reach, or solver result.

The row-capped handoff is executable as `h7_staging_truth_export.sql`. Query
`01c6bfda-0821-a0dc-006c-c703088d161e` completed in 10.305 seconds and
returned 71 distinct accepted loan/document rows, carrying 626 distinct BBL
edges with per-subject cardinality 2–172. A `RESULT_SCAN` validation found zero
row-contract mismatches, row-cap failures, BBL-count mismatches, non-multi-BBL
rows, missing MASTER provenance, insufficient LEGALS provenance, invalid round
PARTY witnesses, or PARTY leakage into the non-round plane. The file SHA-256 is
`230e40407e805e0ec4783185dcd731edb2285553051c91f69e726dd32aea13e1`.
Nesting sorted BBLs at accepted-loan grain prevents the 172-BBL subject from
being clipped by a 200-row transport cap; it does not turn source-row count
into independent information. The export intentionally omits MapPLUTO
candidate parcels, so it proves accepted legal truth and provenance—not
candidate reach, solver correctness, or `live_complete` status.

Candidate reach is now measured independently by
`h7_staging_halo_reach_control.sql`. A direct formulation that recomputed H3
over both pinned parcel releases was cancelled as
`01c6bfe1-0821-a0dc-006c-c703088d1642` and not repeated. The repaired
`STG_GEO_GEOMETRY_HOT_KEYS` path exposes 856,614 and 856,687 valid populated r8
MapPLUTO keys for 26v1 and 26v2. The checked-in control now parameterizes the
halo. File-backed k1 query `01c6bff9-0821-a6c8-006c-c703088d25d2`
completed in 3.218 seconds and k2 query
`01c6bff9-0821-a6c8-006c-c703088d25d6` in 4.557 seconds; all eight emitted
guards were `ok` and the selector string was bound to the actual halo.

For both releases, non-round subject reach is 24 full / 2 partial / 9 none and
round reach is 28 / 1 / 7. The combined 52/71 full-reach count is a candidate
channel result, not solver accuracy. The two releases have the same subject
status counts and truth-edge hits in this slice, while their candidate
cardinalities differ slightly; this is scoped equality only.

Increasing the halo to k2 did not recover one additional accepted legal-truth
edge or subject. It instead raised median section candidates from 5,758 to
14,449 non-round and 4,931 to 10,992 round, with section maxima of 31,631 and
29,788 and loan-union maxima of 50,035 and 58,184. This is useful negative
evidence: the remaining legal-truth misses are not repaired by one more H3
ring and should be investigated as association/geocode/discriminator failures,
not paid for with ever-wider solve regions.

The execution geometry rejects a monolithic interpretation. Across 101
non-round and 63 round point-owned r8+k1 sections, median parcel counts are
5,758 and 4,931, p90 is 12,576 and approximately 9,652, and the maximum is
13,663. One non-round section is empty. Unioning a loan's sections yields
1,192–27,120 parcel candidates, so those unions are reach diagnostics only.
Exact solving still requires each bounded section to be reduced through its
evidence-incidence components and then solved as small residuals. Snowflake H3
is empirical blocking in this receipt; h3o replay remains required before the
cells can be admitted as canonical home-cell artifacts.

The point envelope also makes the truth-plane boundary visible: non-round
collateral points extend to latitude 42.913397 and longitude -75.596272 even
though the loan was admitted through raw NYC filed county plus ACRIS legal
borough. A geocoded point can therefore explain a reach miss but cannot negate
the accepted legal truth. The source SHA-256 is
`6eaec54140218e1ecb8154abb76fe770b26737d1abe6c0dada3a71a4a2368dee`.

An address-independent alternative is now measured by
`h7_staging_pip_block_reach_control.sql`. It finds every pinned MapPLUTO parcel
containing a collateral point, expands only to parcels sharing that parcel's
six-digit BBL block, and constructs the candidate relation before accepted
truth is flattened. Query `01c6c11c-0821-a0dc-006c-c703088da762` completed in
15.612 seconds with all eight release/plane/association guards `ok` and zero
reach-accounting failures. The containing-parcel step reached 158/168 points.
Both releases then produced 38 full / 15 partial / 18 none over the 71 accepted
subjects. Candidate sets range from 0 to 755 BBLs, versus 1,192–27,120 for the
diagnostic H3 loan unions, but full-truth reach is lower than H3 k1's 52/71.
This is a real size/recall tradeoff: block expansion is a useful bounded
candidate-strategy baseline, not evidence that a smaller candidate set is
better or that it should replace tile ownership.

One accepted subject has zero containing parcels and therefore zero block
candidates. H.7 now keeps such a release row and its `reach=none` denominator
instead of refusing the empirical result, but excludes it from the exact
default parcel-profile solver population; parcel-grain composition still
requires a nonempty parcel candidate universe. The explicit building profile
removes the old parcel-oracle dependency only for building-grain questions. A precursor query
`01c6c11b-0821-a6c8-006c-c703088db796` is discarded because candidate-row
multiplication reported the impossible 75 reached truth edges out of 73. The
corrected query uses distinct truth membership plus an explicit reached≤truth
sanity check. Its source SHA-256 is
`26d77c2eb78740c60d386c372d0e2c3fa8a7f049ff3c089ffc485a92e37a39b4`.

A file-exact comparison now places that PIP-block selector beside H3 r8+k1 on
the identical 71-subject × two-release denominator. Query
`01c6c14f-0821-aa0e-006c-c703088dc33a` returned 24 guarded rows: three
selectors × two releases × four truth/association strata. Each selector saw
all 626 legal-truth BBL edges per release. H3 r8+k1 reached 208/626 and
classified 52 full / 3 partial / 16 none; PIP-block reached 186/626 and
classified 38 / 15 / 18. Their union had the H3 result because every PIP-block
candidate in this cohort was already present in H3. The union row is only
counterfactual reach accounting. It is not a 71-loan union solve, and the
smaller PIP candidate sets do not compensate for their lower recall.

`h7_staging_pip_block_population_export.sql` carries this bounded candidate
channel into a row-grain handoff. File-exact query
`01c6c174-0821-aa0e-006c-c703088dc742` completed in 43.370 seconds and emitted
142 rows: exactly 71 accepted loans × two pinned MapPLUTO releases, including
two explicit zero-candidate release rows and no guard rows. It preserves
candidate BBL/source-row/geometry-digest arrays and the accepted ACRIS source
locators, but it remains a raw staging contract rather than a typed Canon
request. This superseding execution also rejected any accepted-truth row
outside ACRIS `2026-08-10` or property state `NY`; its source SHA-256 is
`d3e287532a83da6b66d0250eb5c6e71d29a088c990b34a7a997eef0121f10e77`.

`h7_staging_source_record_bytes_export.sql` performs the next, still
profile-specific handoff. Its derived Canon key/value payloads bind each source
role, source locator, source vintage, and parcel edge before base64 encoding;
they are not full or original warehouse rows. Live role diagnostics under query
`01c6c180-0821-aa0e-006c-c703088dc906` found zero role, parcel-union,
locator, hash, or NY-scope failures. Payload aggregate query
`01c6c189-0821-a0dc-006c-c703088de03e` covered all 142 release rows / 71 loans,
with 5–817 derived records per row, a 1,353-byte maximum record, an 876,919-byte
maximum row payload, and the same two zero-candidate rows. After that execution,
the final projection alone was extended with all eight accepted-plane
denominators required by the adapter; therefore the current SQL SHA-256
`d806b0949cbcc2dd6a66817529de8efd72cf733cc87f5304d8b19e9e23f174c8`
is statically checked but is not claimed as file-exact live execution.

`canon geo materialize-h7-staging-batch` parses the Snowflake-shaped batch,
rejects guard rows, mixed metadata, release or denominator drift, malformed or
wrapper-inconsistent derived payloads, and then delegates to the existing typed
H.7 population materializer. The warehouse wrapper projection is not required,
however. `canon geo materialize-h7-pip-block-batch` consumes the preceding
candidate rows directly and deterministically derives source-record digests
from their exported bridge, ACRIS, and MapPLUTO locators and hashes. This is the
positive offline warehouse-row → typed H.7 handoff even when the MCP success
envelope omits a durable query id. Such a run is explicitly
`observed_snapshot`/`observed`: Canon content-binds the typed rows, requires a
nontruncated positive diagnostic execution receipt, and refuses to promote the
result to `LiveComplete`. Derived payloads are not original source bytes, source
count is not independent evidence, and a query-text/result digest is not an
executor-issued query identity. Both H.7 commands are NYC measurement-profile
adapters, not generic-engine vendor branches.

The first full current-build run of that direct adapter consumed the fresh
142-row MCP result for bridge build
`ce3953ac-c2d4-4b48-bf02-29f0cf341389`. Canon emitted
`canon_geo_h7_population.v0` with typed-row BLAKE3
`ef75594e16ced75a24a6392dfdfc4776e1c2dc7a69b01caf889ee6f256817541`:
71 unique accepted subjects, 35 non-round plus 36 round exact-lender, and 70
primary-release solver subjects. The missing solver subject is not dropped
from evaluation accounting: it is the one loan whose two release rows have
zero candidates, hence `reach_status=none`, and an empty candidate universe is
not fabricated as a solve. PIP-block reach remains 38 full / 15 partial / 18
none at logical-subject grain; the artifact reports 76 / 30 / 36 when both
release rows are counted. The MCP success envelope supplied no query id, so
this result is `observed_snapshot`, not `LiveComplete`; it adds no subjects to
the frozen 79-case denominator and is not solver-accuracy evidence.

Feeding the emitted primary-release population immediately through
`canon geo evaluate` makes the remaining gap precise. All 70 nonempty
candidate cases produced complete exact residual artifacts with zero assignment
budget failures and zero component-budget fallbacks. Candidate reach among
those solver inputs was 38 full / 15 partial / 17 none; only the 38 full-reach
cases were truth-scored. But every case reported `evidence_no_observation`, so
the solver returned 69 ambiguous abstentions and one structural singleton, with
zero false merges and zero backbone true positives. The singleton is not an
accuracy success because its truth is unreachable. This is positive solver and
typed-handoff operation, not evidence-driven resolution: the next E4 step is
to compile independently admissible address/size/footprint/deed observations
onto these candidate universes without importing the held-out ACRIS truth.

That formerly manual boundary is now a compiled, source-neutral path:
`canon geo stack-evidence` accepts truth-blind overlays, emits a replay-validatable
population stack, supports exact idempotent restacking, and feeds `canon geo evaluate`
directly. Its adversarial contract tests prove that overlays cannot carry truth, renamed
semantic duplicates cannot inflate evidence, stale base-evidence bindings refuse, and
contradictory hard overlays produce an honestly empty feasible set. This establishes the
mechanism for accretive evidence-driven resolution; it does not supply or validate the
missing live address/size/footprint/deed observations, improve the measured H.7 reach
denominator, or make the frozen E4 gate green.

The five-case E4 deficit was also tested rather than papered over. The
address-independent consensus-document probe admits an otherwise ambiguous
loan only if every candidate document has complete legal rows and the same
multi-BBL set. Query `01c6c162-0821-aa0e-006c-c703088dc4c6` completed in
35.426 seconds with both plane guards and denominator identities passing, but
admitted zero new subjects. Missing legal rows or non-multi-BBL candidate
documents eliminated the ambiguous loans. The frozen E4 denominator therefore
remains five genuine nonduplicate cases short; duplicate releases, truth-plane
replays, or relaxed admission do not fill it.

The next bounded boundary is executable as
`h7_staging_incidence_shard.sql`. All 16 deterministic shards completed in
12.452–17.931 seconds each and returned 88 distinct r8+k1 center sections.
Aggregate receipt `01c6bff7-0821-a6c8-006c-c703088d25c2` reconciled 497,128
parcel memberships plus 176,086 raw observations. Section work units contain
5–17,617 nodes (median 6,987; p90 13,219.6), but every section's component
median is 1, the median section p90 is 3, and the maximum observed raw
predicate-incidence component is 109. There were zero multi-majority
observations and zero component-shape or accounting failures.

The wider sample also corrects the earlier favorable shard-0 halo result.
Eight raw observations in three sections have their unique complete-reference
majority parcel outside k1. Diagnostic query
`01c6bff8-0821-a0dc-006c-c703088d1682` placed all eight parcel home cells in
ring 2. They are predicate-reach misses, distinct from the unchanged H.7 legal-
truth reach under k2. One additional section has five observations but zero
MapPLUTO work parcels: it is the collateral point at 42.913397, -75.596272,
not an NYC parcel section. Exception receipt
`01c6bff7-0821-a0dc-006c-c703088d167e` preserves all four affected sections.

This is positive evidence for the proposed scaling argument, not its proof.
It now covers every accepted-subject point-owned center, but uses Snowflake
`GEOGRAPHY` rather than Canon's exact local integer predicates and treats NYC
and Overture records as raw observation nodes rather than reconciled latent
buildings. Overture frequently carries OSM lineage, so its agreement with NYC
is not automatically independent evidence. Additional constraints can also
couple these predicate stars in the solver graph. The first ad hoc attempt is
discarded because a raw-number/text BBL mismatch produced zero local parcels;
the file-backed query normalizes the raw `.0` suffix and separately requires a
nonzero work unit. Exact per-shard query IDs are retained beside the script in
the measurement README. Its source SHA-256 is
`d289cc42f742cdfb2b009a8630b10a9122d22fe8c9faa5fd8d71ff94c26734e1`.

These are PIP baseline measurements against document truth, not solver correctness or a
release precision claim. Candidate-reach failure remains upstream of solver truth; human
adjudication of the contested strata remains open. Exact query IDs, denominators,
provenance receipts, borough and accuracy strata, discarded partials, and the bounded SQL
shape are recorded in the bd-179b report.

---

# Appendix I — WORKED: the six-case corpus exists; the code gate is satisfied

Added 2026-08-16 (bd-tccn; index at `docs/geo_design_session/CASES_INDEX_BDTCCN.md`, one
file per case with structured evidence tables and exact SQL). The operator's 2026-08-14
"no code until this exists" gate is met: six cases, query-selected (not hand-curated),
worked end to end from landed data, each forcing a distinct design decision:

| # | Property | Verdict | Decision forced |
|---|---|---|---|
| 1 | 1 Grace Court, BK | singleton, 4 sources converge | the clean floor + ablation control; even the sibling "1 GRACE CT" spelling fails naive normalization |
| 2 | 982 Madison St, BK | singleton via address after geocode abstains; nearest-lot picks the wrong building | tile-bounded proximity; **no snap-to-nearest** |
| 3 | 107–111 N 9th St, BK | three-parcel assemblage | interval semantics; **one BBL is a false answer** |
| 4 | 199–205 First Ave + 349/351 E 12th, MN | six-parcel core; parsed "199 E 12th St" rejected as synthesized | multi-address parsing; **chimera rejection before geocode trust** |
| 5 | 66 Crosby St a/k/a 514 Broadway, MN | singleton despite zero address matches; ACRIS carries both frontages | address disagreement is noise; **a/k/a fields are address sets** |
| 6 | 305 E 72nd St, MN | parcel singleton, building residual {2 BINs} | **entity-level output**; parcel identity cannot answer the product question |

Recurring acquisition finding: cases 4, 5, and 6 each hit the missing address-set layer
(only primary addresses in MapPLUTO) — the bd-35qg/PAD elevation from §12, now with three
worked receipts. The case-1 source snapshot recorded FEMA NY 5.0M and Microsoft GlobalML
NY 5.4M rows landed while **Overture reported 0 NY rows**. That was an historical
2026-08-16 landing gap, superseded for the bounded six-stratum population by the
2026-08-30 F.6 Overture measurement; it must not be read as current source state or as a
complete NYC Overture denominator. These six artifacts are the seed of the
`--suite`/`--gold` evaluation corpus, the visual evidence card's worked examples
(bd-101v), and a showable pre-product sales artifact.

---

# Appendix J — VERIFIED: Appendix A.5's imagery table, from primary sources

Added 2026-08-16 (bd-q5k2; full dossiers with license quotes, live-check receipts, and
URLs in `docs/geo_design_session/IMAGERY_SOURCES_BDQ5K2.md`; access date 2026-08-16).
Every A.5 candidate dispositioned from agency pages, license text, and live
bucket/STAC/range-request checks — not model recall. What changed from A.5's assumptions:

1. **NYS/NYC orthoimagery is the best first source for NYC, not NAIP** — 6-inch true
   orthos, verified borough downloads for every even year 2006–2024, permissive
   (NYS open / NYC CC BY 4.0), byte-range-capable with ETags. BC/FP/CD all strong at
   biennial cadence.
2. **NAIP's posture changed:** AWS buckets are now Requester-Pays (A.5's "no key, no
   rate limit" no longer holds); the anonymous path is the Planetary Computer COG mirror
   (206-verified). The A.5 vintage discrepancy resolves to **2010–2023**, with NY flown
   only in odd years plus 2022. Demoted to national fallback.
3. **3DEP survives with a sharpened claim:** public EPT bucket live (NYC = 4.75B points),
   but Times Square's product is the **2013/Sandy-era** collection — measured height,
   decade-old. The "no model to characterize" hope is refuted as stated and replaced with
   the defensible version: deterministic extraction whose error is characterizable from
   point density, classification, and footprint comparison.
4. **Sentinel-1/2:** conditional, change-detection only (10 m; BC/FP rejected). **NOAA
   ERI:** survivor for event-scoped change evidence (0.3–0.5 m, bucket live through 2026
   events). **USGS HRO:** rejected as a distinct source (legacy 2000–2016, no clean
   pinnable catalog).
5. **Commercial tier all conditional** — every vendor (Maxar/Vantor, Planet, Nearmap,
   Vexcel, Airbus) permits internal use only under contract-bounded terms; Planet's
   default terms reject local multi-user caching outright. **Basemaps rejected with the
   governing clauses quoted** — Google expressly forbids digitized building outlines;
   Mapbox forbids redistributing offline tiles; Esri offline use exists only inside
   licensed ArcGIS content packages. A.5's "avoid" instinct is now a citation.

Catalog provider/channel registration deliberately deferred (recorded in the report);
the survivors table is the registration-ready input.

---

# Appendix K — MEASURED: cheap tile discriminators do not separate geometry's wrong third; the headroom must come from evidence not yet landed

Added 2026-08-16 (bd-1a12; full tables, exact SQL, failed query shapes, and the
reconciliation diagnostics in `docs/geo_design_session/PLAUSIBILITY_BD1A12.md`). The
sharpest test the plan has faced: Gate V2's labeling (154 known-correct, 79 known-incorrect
PIP answers) made the central premise measurable — can tile-local evidence identify the
~34% of geometry answers that are wrong?

## K.1 The discriminator panel: no clean separation

On the 233 labeled points, every cheap deterministic tile fact either saturates or
false-refutes heavily:

```
  discriminator                    fires correct   fires incorrect   verdict
  NYC footprint on PIP lot           153/154          79/79          saturated
  street match (PIP-anchored)        153/154          78/79          saturated
  house-number in block range        114/154          51/79          catches 35% of wrongs,
                                                                     false-refutes 26% of rights
  house-number agrees with PIP lot   123/154          52/79          weak
  FEMA structure on PIP lot           63/154          28/79          non-separating
  boundary depth < 3 m                74/154          51/79          false-refutes 48% of rights
  parity match (where derivable)      68/154          25/79          no independent power
```

No simple accept/refute rule over these facts beats PIP alone on truth-covered points.
**With currently landed evidence, the deterministic cascade has no measured headroom over
the 66% baseline.** The founding W 74th/W 49th proof case reproduces exactly (the tile
refutes the wrong rooftop point, supports the right interpolated one) — targeted
refutation works; the aggregate rule does not.

## K.2 The strict street-presence predicate is representation-bound

After a definitional reconciliation (the panel's street row and the universe query were
measuring different predicates under one name — caught because the subset and universe
rates could not both be true), the consistent strict form (parsed street matches any
parcel-address street in the centroid-r9+k1 tile) fires on only **10.38% of the full
universe and 15.09% of labeled points** — with the tile join verified non-empty (median
843.5 parcels/tile). The failure is street-*string representation*, not street absence:
MapPLUTO primary-address spellings vs parsed streets defeat a fixed normalizer ~90% of the
time. As a refuter it would abstain on ~90% of everything; operationally unusable — and it
is precisely the gap §7's `regular` address grammar and an address-SET layer (PAD,
bd-35qg) exist to close.

## K.3 What this means for the plan

1. **The kill-criterion gap (66% → better, at ~95% coverage) is not closable with landed
   evidence and simple rules.** The measured paths to headroom: the PAD address-set layer
   (three worked-case receipts in Appendix I), document evidence (ACRIS, per Appendix H),
   and grammar-level address matching (`regular`) — all pre-identified by the plan, now
   with measured justification instead of argument.
2. Trap for all future H3 SQL, measured: stored `H3_R8` equals direct centroid-r8 on
   856,614/856,614 parcels, but `H3_CELL_TO_PARENT(centroid_r9, 8)` disagrees on 61,607
   (7.2%) — H3 child/parent nesting is not spatially exact; never mix the two forms in
   one predicate.
3. The plan's self-auditing pattern fired again, one level up: the apples-to-oranges
   predicate was caught by a subset-vs-universe reconciliation check, not by inspection.
   **Predicate definitions are load-bearing** — the same lesson as Appendices D, F, and H,
   now with its fourth receipt.

## K.4 Coda — what this appendix does and does not falsify (added 2026-08-16, on review)

This appendix's original framing overstated. What K measured is that **each tile signal is
individually weak as a unary accept/refute rule** — which is §2.1's few-bits premise,
measured, not the architecture's thesis, refuted. The plan never claimed any single
discriminator separates; it claims weak constraints **sum under joint propagation over the
candidate set**. That summing test — §17's E3 pairwise candidate test — has not been run,
and K's panel design (independent 2×2s on the predicted answer only) could not have run
it. K stands as: (a) a characterization of the input signals, (b) proof that no cheap
shortcut around the full machinery exists, and (c) the event that forced §16 and §17 into
the plan. The headroom question remains open until E3 answers it.

---

# Appendix L — MEASURED: E1–E3 ran; the failure mass was never a ranking problem

Added 2026-08-16 (bd-3ab6 + bd-2qjj; full tables and exact SQL in
`docs/geo_design_session/E1_E2_TAXONOMY_ATTR.md` and `E3_PAIRWISE.md`). The §17 ladder's
first three stages, run on the Gate V2 labeled set (233 PIP-covered points: 154 correct /
79 incorrect, denominators independently re-reconciled by both agents), interpreted
jointly per the §17 gate.

## L.1 The taxonomy (E1): two classes own 91% of the failures

```
  class                              points   signature
  gross geocode error (>500m)          40     avg 7.1 km, max 23.3 km from true parcel
  condo representation residue         32     PIP-parcel-to-true distance 0.00 m
  assemblage-neighbor artifact          2
  adjacent-lot near miss                2
  residual truth-contamination          3
  (sums to 79; sanity arm: 0/154 correct points classified)
```

The gross class is unambiguous wrong-location input — the true lot is kilometres away and
**not in any tile the point defines**. The condo class is the opposite: **geometry found
the right building** (distance 0.00 m); ACRIS records unit BBLs that MapPLUTO does not
carry as parcels. Neither is a candidate-selection failure. Only 4 points (2+2) are cases
where re-ranking within the tile is the relevant tool.

## L.2 The attribute channel (E2): thin on this proving ground

Inventory row 6's first exercise: filtered to genuinely SF-denominated assertions
(`SIZE_MEASURE='SQFT'`), coverage collapses to 10 correct / 17 incorrect labeled points —
NYC CMBS skews multifamily and asserts UNITS, and MapPLUTO landed no unit-count
comparator. On the handful of comparable rows the band test does not separate (and
mildly favors the wrong lot, n=5). Row 6 is recorded as sparse-here, not dead — its
density is geography- and asset-class-dependent, an E5 question.

## L.3 The pairwise test (E3): blocked by candidate reach, honest on both denominators

```
  all 79 failures (out-of-scope counted as unsolved):  true-lot wins 0/79
  7 tile-addressable failures:                          true wins 0, ties 0, PIP wins 7
  control arm (76 matched correct points):              true lot beats same-tile neighbor 76/76
```

The scope split is the finding: 31/79 selected true BBLs are absent from MapPLUTO
(the condo residues), 41 are present but outside the r9+k1 tile (the gross errors) —
**72/79 failures are unreachable by any tile-local pairwise solver by construction.** Of
the 7 reachable, joint measured non-ACRIS evidence never ranks the true lot first (2/7
had an individual true-winning row; the vote still went to PIP) — and 3 of those 7 are
contamination suspects where "losing" may be correct behavior. The control arm passed:
the scoring machinery correctly prefers the true lot 76/76 when it is the answer. The
method works; there is almost nothing in this failure population for it to fix.

## L.4 The verdict under §17

Read strictly, the kill condition fires **for candidate re-ranking with currently landed
evidence**: the bits do not need to sum because the failures re-ranking could address
barely exist. But the taxonomy dissolves the premise rather than the plan: the measured
path from 66% precision to the mid-90s is

1. **a condo/ledger representation bridge** (32 points, deterministic — billing↔unit BBL
   mapping; the generic class is "ledger representation compilation," squarely canon's
   identity-compiler competence, no solver required);
2. **refutation/abstention on wrong-location input** (40 points — bd-1a12's capability
   with asserted-street semantics, strengthened by an address-set layer; the tile
   proves the answer is absent and abstains rather than answering);
3. **honest residual** on the remaining handful (doubletons, contamination suspects).

This reweights the architecture's role on the point-resolution task from candidate
selection toward **justified abstention and representation compilation — which are §9.1's
own claimed products.** What the labeled set cannot exercise, and therefore remains
genuinely open for the constraint machinery, is the *collateral-composition* question —
which parcels and buildings constitute the property (Appendix I cases 3, 4, 6; the 79
non-condo multi-BBL loans of H.4) — plus E4 (joint propagation, now pointed at
composition rather than point re-ranking) and E5 (the genericity gate).

Ladder status: E1 ✓, E2 ✓ (sparse-here), E3 ✓. E4 and E5 remain, re-aimed by these
results.

## L.5 Operator doctrine, incorporated (2026-08-16)

Two corrections from operator review of L.4, both now binding:

1. **Abstention is a reacquisition trigger, not a terminal state.** When the tile refutes
   its input ("this address is nowhere in here"), that is a signal to *re-geocode and
   retry in the right tile* — a bounded, deterministic outer loop, with each pass pinned
   like any other run. The 40 gross-error points are therefore not merely
   honestly-abstained; they are **recoverable**, and the recovery rate of the
   abstain→re-geocode→retry loop is a measurable number (each retry re-enters the normal
   pipeline; nothing in the architecture changes).
2. **The answer is the best-supported entity, not a ledger form.** A BBL is one alias of
   the property entity, in one ledger's representation. When the unit-BBL form is
   unavailable or mismatched, delivering **the building (BIN) or the parcel** is a valid,
   valuable answer — "get what you can get," stated at its claim class. Consequence for
   measurement: **all precision numbers must be scored at entity grain, not ledger
   grain.** Appendix H.6's 66% is a ledger-grain number; the 32 condo residues (right
   building, distance 0.00 m) are *entity-grain correct*. The entity-grain re-score of
   the labeled set is the immediate follow-up measurement; predicted shape: ~80%
   entity-grain precision before abstention, mid-90s on answered points with the gross
   class abstained-for-retry.

This aligns the scoring with what §16.1 and Case 6 already said the output is: parcel
singleton, building residual, each level stated — never a forced collapse to one ledger's
key.

## L.6 MEASURED: the entity-grain operating point

Added 2026-08-16 (bd-s3i9; full tables and exact SQL in
`docs/geo_design_session/ENTITY_GRAIN_RESCORE.md`). L.5's two doctrines, applied to the
labeled set with a recorded predicate (`entity_correct := ledger hit OR E1
condo_representation_residue` — ACRIS condo-unit truth, no ledger hit, missing MapPLUTO
unit geometry; parcel and building grain are one predicate until a unit→BIN crosswalk
lands):

> **CURRENT STATUS — MEASURED, PROVISIONAL, AND TRUTH-INSTRUMENT-LIMITED.** Appendix M
> supplies independent PAD evidence that much of the gross class may be Gate V2
> contamination. Treat the operating point below as a conservative experiment result, not
> a release-quality precision estimate; the lender/party truth-gate rebuild remains open.

```
  scoring                                        precision
  ledger grain (H.6, unchanged)                  154/233 = 66.09%
  entity grain (parcel/building)                 186/233 = 79.83%   (all 32 condo flips)
  entity grain, gross class abstained-for-retry  186/193 = 96.37%
  ... excluding 3 contamination suspects         186/190 = 97.89%
```

**The plan's honest operating point, with no new machinery, is ~96–98% precision on
answered points** — geometry plus the representation doctrine plus abstention-for-retry.
The residual wrong answers are the 4 genuine ranking cases (E3's domain) and the
contamination suspects.

**The retry loop needs fresh acquisition, measured:** of the 40 abstained gross points,
11 have already-landed alternate geocode rows, but **0 land in a different r9 tile and 0
PIP into an ACRIS truth block** — the recovery ceiling from landed data is 0/40. Retrying
requires a new geocode pass or entry through the address channel (PAD, bd-28kn); it is an
acquisition step, not a re-read.

Caveats that travel: truth coverage remains the Gate V2 slice (5.94% of points,
non-round-amount biased); the condo flip is predicate-granted, not independently
adjudicated (Source 2 of bd-179b remains the check); and abstention's *coverage* cost is
40/233 ≈ 17% of truth-covered points parked for retry. Within those bounds, §13's
commercial claim now has its first defensible shape: **high-precision answers, honest
abstentions with a recovery path, and residuals that name themselves.**

---

# Appendix M — MEASURED: PAD wired in; K.2 resolved as representation; the gross class re-reads as truth contamination

Added 2026-08-16/17 (bd-3sot + bd-3ujr; full tables and exact SQL in
`docs/geo_design_session/PAD_SCALE_BDNEW.md` and `PAD_LABELED_BDNEW.md`). PAD release
pinned **26B** (2026-05-01). Match predicate recorded in both reports: borough-scoped
SND street-code match with normalized-street fallback, integer range overlap with
parity, display-string equality for hyphenates.

## M.1 PAD at scale: the address channel, replaced

```
  resolution of 5,269 address-county keys:
    naive MapPLUTO exact (Appendix E)      1,522 / 5,269 = 28.89%   0 multi
    PAD range-aware                        3,930 / 5,269 = 74.59%   (2,337 unique,
                                           1,593 multi-BBL = 30.2%, 1,339 unresolved)
```

2,870 keys resolve **only** through PAD (the corner/frontage/range population MapPLUTO's
single address missed). The 30.2% multi-BBL rate is the *honest* address ambiguity the
naive baseline structurally hid — adjudicating it is solver work, not lookup. Queens
hyphenates resolve at 67.2%; the unresolved residual (1,339 keys, incl. multi-address
strings and a/k/a forms) is the parse forest's measured population (bd-158y).

## M.2 K.2 resolved: it was representation, and the refuter is back

The K.2 replay — identical point grain, identical centroid-r9+k1 tile semantics, one
variable changed (lot-side street universe = PAD address sets + SND variants):

```
  strict street presence   MapPLUTO primary (K.2)   420/4,046 = 10.38%
                           PAD/SND (this round)   4,005/4,046 = 98.99%
```

K.2's "90% absence" was ~99% representation artifact. **Street-absence refutation is now
operationally viable**: 41 points in the whole universe fire it — a tiny, high-signal
abstention population instead of a catastrophic one.

## M.3 The address-set assumption, quantified

PAD-native cardinality over 874,168 BBLs: mean 1.52 addresses/BBL (max 2,071);
62.8% single-address, 37.2% multi-address. BINs: mean 1.26/BBL, 25.2% with two or more.
The architecture's "an address is a set" premise is now a measured distribution, and
`NUM_ADDRESSES` asserted-vs-computed sanity (Δ −7,765 on 3,137 BBLs) is recorded.

## M.4 PAD on the labeled set: evidence row, not oracle

On the Gate V2 truth slice PAD standalone is sparse and modest — 82/233 coverage
(35.19%), 43/82 lot/entity precision (52.44%) — and highly asymmetric: in the correct
class it confirms the right lot 42/43; refutation fires 7/79 vs 1/154 false. The condo
crosswalk (Q1b) exists for **31/31** missing-geometry condo points but billing-BBL equals
the PIP lot on only 10/31 — so the entity bridge is **crosswalk + block/geometry
confirmation**, not key equality. PAD enters §16.3 as evidence rows (membership,
refutation, crosswalk), not as a standalone resolver.

## M.5 The reinterpretation: much of the "gross" class is contaminated truth

The round's decisive finding: on gross-class points where PAD resolves, it **confirms the
PIP lot 20/21 times** — the loan's address string, its geocode, and PAD's ledger agree
with each other and against the ACRIS amount+date match. Three independent channels
versus one, on top of E1's contamination signals (29/40). Reading: a substantial share of
the 40 "gross geocode errors" are **residual Gate V2 truth contamination**, not bad
geocodes. Consequences:

1. **L.6's operating point was conservative** — some abstained-for-retry points were
   correct answers scored wrong by bad truth; true precision is likely above 96.37% and
   the 17% abstention cost overstated.
2. **The lender-name truth-gate expansion (bd-179b) is re-promoted** — it cleans the
   instrument every other number depends on.
3. The retry loop's genuine target shrinks toward the truly-wrong-geocode residual
   (the W 49th class), which M.2's revived street-absence refuter now catches cheaply.

Ladder status unchanged (E4 composition, E5 tier-curve pending); every E5 tier that
lacks a PAD-equivalent falls back per the §17 doctrine — coverage narrows, precision
holds, abstention absorbs.
