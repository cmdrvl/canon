# Canon Geo Agent Architecture

> Status: **normative target architecture; partially implemented leaf capabilities**.
> This document defines how an agent should operate Canon Geo as one coherent system.
> The only shipped control-plane command is `canon geo capabilities --emit json`;
> `geo plan`, `geo run`, and `geo inspect` remain proposed/unavailable.
> The mathematical model and empirical gates remain governed by
> [`PLAN_CANON_GEO.md`](./PLAN_CANON_GEO.md).

> **Reuse finding:** Canon contains library-level `canon.project.plan.v1` and
> `canon.project.run.v2` machinery for manifests, locks, DAGs, receipts, resume,
> invalidation, lifecycle, and workspace policy. The public project CLI exposes lock
> refresh and pure planning, while `project run` is validation/reuse-only until typed
> node executors are registered. Geo must extend this substrate, not build a parallel
> orchestrator.

## 1. Purpose

Canon Geo should let an agent answer a bounded location question accurately without
having to memorize pipeline choreography, infer state from filenames, or confuse the
availability of a source with evidence that changes an answer.

The system takes a question, a resolution profile, a resource budget, and either a regional
inventory or enough declared gaps to request one:

```text
question + capabilities + resolution profile + resource budget
  + regional evidence inventory or typed discovery gaps
```

and compiles them into:

```text
deterministic plan -> immutable run revisions -> typed answer + explanation + next action
```

The agent-facing system is successful when an agent can answer, in one bounded inspection:

1. What question is this run answering, at what entity grain and as of when?
2. Which source releases and evidence classes are available here?
3. Which evidence was admitted, kept diagnostic, or rejected, and why?
4. Did the true candidate have a route into the bounded universe?
5. Which local components were solved exactly, approximated, or left unresolved?
6. What is forced, ambiguous, contradictory, or unsupported?
7. Which prior work is reusable after new evidence arrives?
8. What is the cheapest next action that can materially change the answer?

No agent should have to reconstruct those answers by reading logs or joining unrelated
artifacts manually.

## 2. The system in one equation

For question `Q`, profile `P`, regional inventory `I`, budget `B`, and admitted evidence
set `E`, Canon Geo constructs bounded sections `S` and returns:

```text
Plan(Q, P, I, B)
  -> Sections(tile + controlled halo)
  -> IncidenceComponents(E | S)
  -> ExactResiduals(component, B)
  -> ReconciledAnswer + Evaluation + NextEvidenceOptions
  -> reviewed registry proposal -> exact replay
```

National evidence volume affects acquisition and indexing. It must not become the variable
count of one solve. Exact reasoning occurs only over incidence components inside bounded
sections. A plan or status surface that describes a national or 500,000-candidate
monolithic solve is invalid.

## 3. Non-negotiable separations

The control plane exists primarily to keep these distinctions visible.

| Plane | Question it answers | What it must never imply |
|---|---|---|
| Availability | Is a source release present for this region and time? | That any target candidate is reachable |
| Coverage | Does the declared subset predicate cover the bounded question? | That a row is correct or useful |
| Candidate reach | Can the labeled or intended entity enter the candidate universe, and against what bounded reference is that known? | That the solver will choose it; that unlabeled production reach is empirically proven |
| Admission | Does a versioned `rho` justify a logical constraint? | That the source is world-truth |
| Constraint effect | How did the admitted constraint change the model set? | That source count is independent information |
| Solver correctness | Is the residual exact relative to the quantized representation and budgets? | That the representation equals world-truth |
| Reconciliation | Did independently solved sections produce one confluent owned decision? | That matching payload digests prove payload semantics |
| Truth quality | Does the answer contain independently adjudicated truth? | That candidate reach or solver exactness is precision |
| Resource cost | What rows, bytes, components, assignments, and explanations were consumed? | That an unmeasured national runtime follows from a tile estimate |

Every evaluation artifact reports these planes independently before presenting any combined
summary. In particular, candidate-reach failure is upstream of solver correctness and must
not be scored as a solver miss.

### 3.1 Optimization is policy-constrained, not a hidden score

“Best” is not defined until the caller declares what losses matter. The exact residual and
hard epistemic invariants are policy-independent. Decisions about additional acquisition,
acceptable abstention, soft ranking, latency, or spend are not.

The question may reference a versioned decision policy declaring:

- requested claim classes and minimum truth/coverage gates;
- relative losses for false inclusion, false exclusion, abstention, and delay;
- whether soft-ranked output is permitted;
- acquisition and compute cost units;
- deterministic tie-breaking and stopping policy.

No policy may authorize an unsound hard admission, relabel an inexact residual as exact,
or trade precision away merely to increase coverage. If no calibrated loss model is
provided, Canon returns a Pareto frontier and dominance relations rather than claiming one
action is globally optimal.

### 3.2 Property vocabulary is not interchangeable

Every artifact uses these properties precisely and reports them separately:

- **deterministic**: the same declared inputs and policy produce byte-identical semantic
  output;
- **confluent**: permitted evaluation or worker order does not change the semantic result;
- **sound**: under the declared `rho` premise, admitted constraints retain the true world;
- **complete**: all candidates or models required by the declared scope and budget are
  represented;
- **canonical**: the representation has a unique normal form under its frozen order/vtree
  and serializer.

None implies another. A deterministic solver can be incomplete. A confluent fixpoint can
operate over an unreachable candidate universe. A sound constraint set can become empty
when a source violates its contract. An exact model count can describe a quantized model
that is not world-truth. Inspection must never compress these into one `exact` or `healthy`
boolean.

## 4. The abstraction tower

Every artifact declares its dependencies explicitly. Higher layers consume typed lower
interfaces; lower layers do not learn the names or assumptions of higher-level
applications.

### 4.1 Geo question

`canon_geo_question.v0` is the complete answer contract:

- stable question id and subject bindings;
- requested answer grains, such as site, property, parcel, building, unit, address, or POI;
- geography and bounded spatial scope;
- explicit query-as-of domain when temporal facts may constrain the answer;
- requested claim classes and presentation limits;
- abstention policy;
- optional versioned decision-policy reference; absent policy limits recommendations to
  dominance and a Pareto frontier;
- resource budget reference.

The question names desired entity grains, not required vendors. A parcel answer may be
unsupported in a parcel-free geography while a building or site answer remains possible.

### 4.2 Capability inventory

`canon_geo_capabilities.v0` is compiled into the binary and answers what this Canon build
can consume and produce:

- supported contract versions and JSON schemas;
- entity levels and relation types;
- evidence classes and `rho` contract families;
- geometry representations and predicates;
- candidate, component, solver, and explanation limits;
- installed resolution-profile and adapter interfaces;
- implemented, diagnostic-only, and unavailable capabilities.

It contains no credentials, live catalog claims, or vendor-specific default authority.

### 4.3 Regional evidence inventory

`canon_geo_regional_inventory.v0` says what evidence is actually available for this
geography and time. Each source instance declares:

- source and release identity, content digest, valid/transaction time, and lineage;
- native entity level or observation-only status;
- evidence classes it can emit;
- coverage/subset predicate and known gaps;
- acquisition recipe reference and local artifact state;
- coordinate/geometry contract and pinned transform where applicable;
- license and egress restrictions;
- declared record and byte estimates.

Availability belongs here, not in core branches. `MapPLUTO`, a county assessor parcel
file, and a licensed client parcel layer are different instances of a parcel evidence
class. A region may declare no parcel instance at all.

### 4.4 Resolution profile

`canon_geo_resolution_profile.v0` turns the question and inventory into permitted
semantics. It declares:

- entity levels available in this profile;
- legal same-level and cross-level relations;
- required, optional, and forbidden evidence classes per requested answer grain;
- source-independent candidate strategies;
- admissible `rho` contracts and diagnostic-only observations;
- deterministic component and solver policy;
- truth planes and evaluation denominators;
- abstention and stopping rules.

Profiles are where geography or use-case differences compose. They may be distributed as
versioned packages. Canon core dispatches typed classes and operators; it never dispatches
on `source_name == ...`.

The current H.7 NYC staging adapter is a profile-specific adapter into generic artifacts,
not the model for the core API. The current composition v0 parcel/building requirement is
a known implementation limit, not an architectural requirement.

### 4.5 Discovery and acquisition handoff

An agent may begin with an unknown region. Canon expresses what it needs through
protocol-neutral `canon_geo_discovery_request.v0` and `canon_geo_acquisition_request.v0`
artifacts; an external executor may satisfy them through Reveal catalog discovery,
Snowflake, S3, local files, or a future service. Canon never embeds the executor, its
credentials, or its network behavior in the deterministic build.

Discovery proceeds from cheap metadata to bounded evidence:

1. identify candidate datasets by evidence class and entity level;
2. list and describe the chosen tables/artifacts before data queries;
3. pin releases and temporal scope explicitly;
4. prove column readability with a bounded real-column read, not metadata counts alone;
5. acquire only the cells, rows, and columns required by the plan;
6. return a typed receipt with query/request id, pagination state, row count, denominator,
   bytes, source/result digests, and proof class.

`ZERO_ROWS`, `TIMEOUT`, `CANCELED`, `PARTIAL`, and `UNREADABLE_COLUMNS` are different
terminal states. A zero result can be a valid coverage finding; inability to execute is
not. A gate that needs positive capability may explicitly require a nonzero result.
Fixtures and retained receipts can validate contracts but never become fresh live proof.

### 4.6 Deterministic plan

`canon_geo_plan.v0` is a typed Geo semantic overlay over the generic
`canon.project.plan.v1` DAG, compiled from the question, capabilities, inventory, profile,
and budgets. The generic substrate owns node identity, dependencies, cache/side-effect
metadata, lock bindings, workspace policy, review/mutation gates, and DAG validation. The
Geo overlay owns entity/evidence semantics, gate planes, section/component identity, and
claim effects. There must not be two independent schedulers or caches. Each node states:

- exact inputs and expected output contract;
- preconditions and gate plane;
- semantic command invocation or external acquisition request;
- deterministic resource limits and estimated resource range;
- cache key and invalidation inputs;
- whether its result can change a requested claim;
- success, abstention, contradiction, and budget-fallback transitions.

Planning is read-only and offline. It must stop before expensive work when the requested
grain is unsupported, the coverage predicate cannot include the subject, or no downstream
claim can change. Missing network inputs become typed acquisition requests; Canon does not
hide network access inside a deterministic provider build.

### 4.7 Source adapters and normalized observations

Source-specific code ends at the adapter boundary. An adapter converts pinned local bytes
into generic typed observations with source locators, units, entity level, time, geometry
bindings, and lineage. It may not:

- assign source authority globally;
- turn repeated rows into independent votes;
- silently convert asserted area into computed geometric area;
- project a time-bounded fact into a timeless constraint;
- invent a stable alias for a source that has none;
- call the network during materialization.

Numeric values carry semantic field id, unit, value origin, and quantization/calibration
bindings. Asserted source area and area computed from canonical geometry are different
types and are never silently summed, substituted, or used as each other's denominator.
Exact unit conversions use declared rational ratios where available. Geometry receipts
keep source decoding loss, transform disagreement/error, and local lattice snapping as
separate quantities; they are never summed into a fictional single accuracy number.

Adapters should be independently testable against a black-box conformance contract. New
sources should normally require a package plus a regional-inventory entry, not a Canon
release.

### 4.8 Bounded sections and candidate reach

H3 is a blocking and ownership index only. A section is:

```text
center tile + controlled halo + declared coverage predicates + resource ceilings
```

The section artifact records center/halo membership, home ownership, coordinate-envelope
sensitivity, source release bindings, structural candidate-universe completeness relative
to the declared local inputs, and complete-bounded-reference truth reach where labels or an
independent reference exist. Exact geometry decides spatial relations; H3 never does.

Reach states distinguish `PASSED_AGAINST_REFERENCE`, `FAILED_AGAINST_REFERENCE`,
`STRUCTURALLY_COMPLETE_RELATIVE_TO_INPUTS`, and `UNVERIFIED`. A known reach failure blocks
the affected claim and triggers reacquisition/replanning. Unverified truth reach may still
produce an exact residual relative to the declared candidate universe, but it limits the
claim and cannot be summarized as measured accuracy. A solver cannot recover an entity
excluded by acquisition, coverage, tiling, halo, parsing, or candidate construction.

### 4.9 Constraint IR and incidence factorization

Typed observations are admitted through versioned `rho` contracts into a generic
constraint IR. Every admission is one of:

- `LOGICAL`: supported by a declared invariant;
- `EMPIRICAL`: calibrated on a named population with a falsification rule;
- `DIAGNOSTIC`: retained but unable to restrict the feasible set;
- `REJECTED`: malformed, unsupported, temporally inapplicable, or unsound.

The compiler then builds the actual variable/constraint incidence graph. Work-unit row
count is not solver width. Source reconciliation must occur before raw observations are
miscounted as latent entities. Components, not tiles and never national populations, are
the exact-solving unit.

### 4.10 Exact residual solving

Each component is solved under deterministic, representation-relative budgets. The output
separately reports:

- non-emptiness;
- exact or lower-bound model count;
- complete or partial backbone;
- materialized residual scope and truncation;
- chosen representation and its frozen order/vtree when applicable;
- deterministic budget fallback;
- proof or explanation availability.

Reduced OBDDs are canonical only under a fixed variable order. SDDs require a fixed vtree
and normalization/compression contract. General d-DNNF is not canonical. Search may be the
best representation for small residuals. Selection must follow measured component shapes,
not aesthetic preference.

More admitted hard evidence satisfies:

```text
Models(T and c) is a subset of Models(T)
```

and may make the model set empty. The system must never translate monotone narrowing into
an unconditional claim that “more sources increase confidence.”

### 4.11 Reconciliation and answer projection

Local decisions reconcile under one deterministic owner rule. Reconciliation refuses
missing owners, unavailable members, halo-only claims, and semantically conflicting
payloads. It never resolves a conflict by arrival order.

The answer projects the residual into requested entity grains and claim classes. An
unavailable parcel alias does not erase a resolved building entity. A profile that lacks
the requested grain reports that grain as unsupported while preserving supported answers.

### 4.12 Evaluation

Evaluation reports the planes in §3 with predeclared denominators. Labels never enter
candidate generation, admission, compilation, or solving. Fixtures prove contracts, not
live coverage or precision. Retained measurements remain retained proof and are never
presented as fresh warehouse execution.

### 4.13 Inspection, explanation, and next evidence

The final layer is the agent control surface. It exposes:

- the question and profile;
- current phase and gate states;
- pinned inputs and reusable artifact hashes;
- coverage and candidate-reach gaps;
- admitted/diagnostic/rejected evidence;
- component-size distribution and exactness;
- answer/backbone/residual/conflict state;
- resource estimates, actual deterministic counters, and budget remaining;
- exact next commands or acquisition requests, ordered by declared policy.

The default view is compact. Every summary field links to a typed artifact and can be
expanded without re-running the solve.

### 4.14 Review, promotion, and exact replay

Geo remains a workbench. Its answer is not automatically accepted registry knowledge.
When the requested claim is eligible, the system may project a
`canon_geo_registry_proposal.v0` containing:

- proposed typed entities, aliases, and same-level/cross-level relations;
- valid-time scope and entity lifecycle bindings;
- exact residual/backbone and claim class;
- run, plan, source, evidence, reconciliation, and evaluation hashes;
- review requirements and unresolved alternatives;
- license/egress projection governing what may enter a shared registry.

Exact solving relative to a representation is not an automatic truth attestation. Soft
rankings, incomplete backbones, unreachable truth, unsupported grains, and unreviewed
conflicts cannot auto-promote. Promotion follows Canon audit/review rules and writes
versioned registry knowledge; normal runtime lookup then replays those accepted aliases
exactly. The run manifest records promotion eligibility and the review/promotion artifact
reference, but the control plane does not bypass existing registry governance.

## 5. The run manifest: the agent's durable working memory

`canon_geo_run.v0` is the one object an agent needs to resume work. It is a typed Geo view
over generic `canon.project.run.v2` node receipts, not a second receipt store or scheduler.
It is not a log bundle; it is a validated index over immutable artifacts.

Each revision contains:

- hashes of the question, capabilities, inventory, profile, plan, and budget;
- phase state and gate outcomes;
- source-release and acquisition receipts;
- artifact references with semantic hashes and schemas;
- deterministic usage counters;
- blockers, abstentions, contradictions, and typed fallbacks;
- current answers and their exactness/truth-plane status;
- ordered next actions with preconditions and expected effects;
- dependency edges used for incremental invalidation.

Semantic dependency hashes include declared inputs, policy, node contract, dependency
semantic hashes, output content digests, and deterministic usage counters that control
fallback. Runtime duration, CPU/memory telemetry, observed currency cost, and publication
paths live in a separately integrity-protected observation envelope; changing them cannot
invalidate downstream semantic work. Paths, ambient clocks, observed
execution timestamps, worker order, and machine identity
are operational metadata and do not enter semantic hashes. Declared query-as-of, source
valid-time, transaction-time, and release-time values remain semantic inputs. A phase
commits only after its output validates and hashes. Publication
uses atomic same-filesystem replacement. Re-running the same plan against the same inputs
is idempotent and reuses verified artifacts. An interrupted run resumes from the last
validated phase without repeating acquisition or exact solves.

Long phases emit bounded progress events on stderr or an explicitly requested JSONL
progress stream. Events name the current plan node, last committed artifact, deterministic
counters, and any lock/input wait reason. They never enter the primary stdout artifact or
semantic hash. Cancellation leaves the last validated phase resumable. Capability/help and
read-only run inspection must not wait on unrelated build or work-directory locks.

The run state is a state machine, not a single success flag:

```text
DRAFTED -> PREFLIGHTED -> MATERIALIZED -> REACH_CHECKED -> COMPILED
        -> FACTORIZED -> SOLVED -> RECONCILED -> EVALUATED

Any phase may instead yield:
WAITING_FOR_INPUT | UNSUPPORTED_GRAIN | ABSTAINED | CONTRADICTED | BUDGET_FALLBACK
```

Those states retain every completed reusable artifact and an exact recovery action.

### 5.1 Parallel agents and deterministic convergence

Plan nodes have stable semantic ids and explicit dependencies, so multiple agents may
claim independent ready nodes—most importantly different bounded sections or incidence
components. Coordination state such as worker identity, leases, heartbeats, and observed
duration is operational metadata and never enters semantic hashes.

Workers publish outputs into node-addressed content storage and then propose a manifest
revision. Identical duplicate work deduplicates by hash. Different valid-looking outputs
for the same node/input hash are a determinism failure and refuse reconciliation. A lost or
expired worker can be replaced without changing the plan or answer. Reconciliation and
evaluation nodes become ready only when their declared inputs are complete.

`inspect` exposes ready, claimed, completed, stale, and blocked nodes plus their bounded
resource ceilings. The scheduler avoids duplicate external acquisition and assigns
component work without creating a global solve. Completion order, agent identity, and
parallelism level may change elapsed time, never semantic output.

## 6. Minimal agent command surface

Leaf commands remain independently callable and machine-described. The target control
plane currently ships capability introspection and proposes three orchestration
operations:

```text
canon geo capabilities --emit json
canon geo plan --question Q.json --inventory I.json --profile P.json --budget B.json
canon geo run --plan PLAN.json --work-dir DIR [--satisfy REQUEST_ID=RECEIPT.json]...
canon geo inspect --run DIR [--component ID] [--compare OTHER_RUN] [--recommend-next]
```

- `capabilities` is shipped offline/read-only and answers what this build can do without
  reading a project.
- `plan` validates intent and emits the Geo overlay plus its generic project DAG without
  mutation; it remains proposed/unavailable.
- `run` delegates scheduling, receipts, resume, and workspace safety to the shared project
  substrate; it orchestrates only offline leaf capabilities, accepts explicit responses
  to outstanding discovery/acquisition requests, and resumes by default; it remains
  proposed/unavailable.
- `inspect` is the one-call situation report, explanation, diff, and next-action surface;
  it remains proposed/unavailable.

`inspect` must emit structured next actions containing the exact command, required inputs,
expected output contract, deterministic cost ceiling, and the reason the action can change
the answer. Human prose is a rendering of those fields, not the only representation.

Until `geo plan`, `geo run`, and `geo inspect` exist, agents must use the implemented leaf
commands listed by `canon --describe`; documentation must label those orchestration
commands as planned rather than advertising them as shipped.

## 7. Resource minimization

The planner optimizes work in this order:

1. Reject unsupported answer grains before acquisition.
2. Use catalog metadata and table descriptions before bounded data reads.
3. Reuse content-addressed inputs and prior component results.
4. Test coverage and candidate reach before solving.
5. Materialize only cells and source columns required by the plan.
6. Factor the incidence graph before selecting a solver.
7. Solve only affected components under deterministic count/byte/assignment budgets.
8. Compute expensive explanations or certificates only on demand or when a named gate
   requires them.
9. Acquire more evidence only when it can change a requested claim or diagnose a conflict.

Semantic behavior must not depend on wall-clock time. Wall time and currency are measured
telemetry; deterministic row, byte, variable, state, operation, model, and proof-size
ceilings control fallback. A faster machine may finish sooner but may not produce a
different answer.

## 8. Accretion and invalidation

New evidence creates a new immutable run revision. It never mutates the meaning of a prior
answer in place.

| Change | Minimum invalidation |
|---|---|
| New rows in one source release | Adapter output for those rows; touched home cells and halos; affected components; downstream reconciliation/evaluation |
| New source instance | Inventory/profile compatibility; touched sections; affected components |
| Source release replacement | Every artifact bound to the replaced release, but no unrelated source or section |
| `rho` contract change | Admissions using that contract and their downstream components |
| Query-as-of change | Time-applicable observations, affected components, answer projection |
| Entity-level/profile change | Candidate universe and relations governed by the changed profile |
| Variable order/vtree/backend policy change | Compiled representations using that policy, not source acquisition |
| Presentation budget change | Presentation projection only when exact residual/backbone already exists |

`inspect --compare` explains the delta: evidence added or removed, components invalidated,
model-set changes, backbone gains/losses, contradictions introduced/resolved, claim-class
changes, and resources reused. Adding hard evidence may narrow the residual or expose an
empty set; neither outcome rewrites the prior snapshot.

## 9. Choosing the next evidence

The next-evidence controller has four distinct jobs:

1. repair candidate reach;
2. diagnose an empty residual;
3. separate remaining residual models;
4. raise an answer to the requested claim class.

For a non-empty residual, a prospective observation partitions the current model set by
its possible outcomes. Canon may report counterfactual separation, worst-case remaining
models, redundancy with existing constraints, and cost. Without calibrated outcome
probabilities, it must not call model-count reduction “expected value of information.”
With calibrated probabilities it may report an explicitly named expected model-set
reduction, still not monetary or decision-theoretic value unless the caller supplies a
loss model.

The prospective observation must declare an exhaustive outcome domain and the `rho` each
outcome would induce. Exact separation claims require an exact current residual and exact
outcome partitions. Saturated counts, partial backbones, sampled models, or budget fallback
may produce explicitly bounded diagnostics only; they cannot support an exact ranking.

Recommendations follow declared policy and prefer dominance: if action A costs no more
than B and provides at least as much counterfactual separation in every modeled outcome,
B is dominated. Source count, novelty, and vendor diversity are not substitutes for
conditional information. Without a declared loss model, the controller exposes the
nondominated frontier and does not manufacture a total ranking.

The controller is allowed to recommend **stop** when:

- the requested claim is already forced;
- all affordable actions are redundant;
- the requested grain is unsupported in this geography;
- the remaining ambiguity is honest and no admitted observation can separate it;
- further work would exceed the declared budget.

## 10. Agent API requirements

Every Geo command and artifact must satisfy these rules:

- JSON is the canonical machine surface; prose and tables are projections.
- `--help`, `--describe`, schemas, limits, side effects, and next commands agree.
- Exit semantics distinguish resolved, partial/abstained, refusal, contradiction, and
  deterministic budget fallback where the command contract requires them.
- Stdout carries the primary artifact; diagnostics do not corrupt it.
- Every refusal and nonterminal state includes a machine-actionable recovery command when
  one exists.
- Inputs are validated before expensive work; unknown fields and contract versions fail
  visibly according to the schema policy.
- Collections have deterministic order; paths and ambient execution timestamps do not
  affect semantic identity, while declared source and query time domains do.
- Commands declare offline/network and mutation behavior.
- Long operations expose progress and wait reasons; cancellation preserves resumability.
- Examples include a positive path, a negative path that defeats a naive implementation,
  and the smallest valid artifact.
- The operator manifest exposes implemented versus planned state. Planned commands never
  masquerade as available capabilities.

## 11. Promotion is part of the system, not part of the solver

The final customer value is durable accepted knowledge, not a directory of workbench
artifacts. The control plane therefore makes promotion readiness visible while preserving
the firewall:

```text
uncertain regional evidence
  -> deterministic Geo run
  -> residual/explanation/evaluation
  -> review-gated typed registry proposal
  -> versioned registry
  -> exact runtime replay
```

Existing Geo identity, identifier, client-output, review-card, and pre-resolution Beads own
the promotion semantics. The agent control-plane epic owns only the typed handoff,
eligibility state, artifact linkage, and next action; it must not duplicate or weaken those
owners.

## 12. What this architecture changes

This architecture does not replace the exact solver, evidence compiler, geometry kernel,
tile ownership rules, or evaluation harness. It connects them into a legible operating
system. It also does not replace Canon project orchestration: it extends its generic
manifest/lock/plan/run/receipt/lifecycle/workspace substrate and projects Geo-specific
meaning through typed artifacts.

It changes the build order in five ways:

1. The entity-level/profile contract must become source-generic and parcel-optional before
   E5 can claim genericity.
2. Capability discovery and the question/profile/inventory contracts precede composite
   orchestration.
3. Candidate reach is a first-class plan gate, never inferred from solver evaluation.
4. A resumable run manifest and incremental invalidation prevent repeated warehouse pulls
   and repeated exact solves.
5. Next-evidence selection operates on the current residual and declared costs, making
   evidence stacking deliberate rather than indiscriminate.

The acceptance standard is not that an agent can eventually find every leaf command. It is
that the agent can accurately control the whole epistemic and computational state with one
small, inspectable set of abstractions.
