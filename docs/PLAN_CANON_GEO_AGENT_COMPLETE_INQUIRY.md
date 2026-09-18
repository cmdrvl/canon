# Canon Geo: an agent-complete inquiry, with two first-class entry points

> **Status:** operator-requested product consolidation and implementation roadmap; proposed behavior is not shipped by this document.
> **Date:** 2026-09-18.
> **Implementation baseline reviewed:** `cmdrvl/canon` main at `bbbf9e1643570a5d31974de5f731663b0da427d2` (2026-09-17).
> **Scope of this change:** add this plan only. No runtime code, schemas, existing plans, acceptance gates, or task states are changed.
> **Authority:** [Canon Geo Agent Architecture](CANON_GEO_AGENT_ARCHITECTURE.md) governs the operating model; [PLAN_CANON_GEO](PLAN_CANON_GEO.md) governs mathematics, historical measurements, scope decisions, and E1–E5/G3 gates. This roadmap proposes additive delivery work and identifies scope clarifications to reconcile explicitly before implementation. It does not silently reverse a frozen decision or relabel an old experiment.

## 1. Product direction: consolidate, do not pivot away

Canon Geo should own the geographic inquiry from incomplete observations to the strongest justified answer, not merely the solver in the middle.

The product promise is:

> Here is what the available evidence establishes. Here is what remains possible. Here is the additional evidence that could settle the question, how to obtain it, and the cost and limitations of doing so.

There are **two first-class use cases**, both already important to the operator:

| Entry point | What the caller supplies | What the caller wants |
|---|---|---|
| **A. Known-address corroboration** | An address, optionally a point, name, or other attributes | Evidence about what is at that address, what physical entity it identifies, whether the supplied association holds, and what remains uncertain |
| **B. Region-and-attributes discovery** | A region and a few qualifying attributes; no trusted address is required | Evidence identifying the most likely address or addresses and associated physical entity, with alternatives and the reasons they are preferred |

These are two ways into **one evidence, reasoning, explanation, and acquisition loop**. They are not separate products or separate solver stacks.

A supplied address is an assertion to investigate, not an unquestionable truth anchor. Conversely, discovery must not require an agent to find the address elsewhere before Geo becomes useful. A discovered address may subsequently be corroborated within the same inquiry, without losing how it was discovered or counting the original evidence twice.

A parcel/building/site answer remains useful when no defensible postal address is available. Do not invent an address from the nearest point or confuse a mailing address, entrance, unit, building, parcel, and entire campus. Complete collateral composition is a stronger optional question, not an entry requirement for either use case.

**The next milestone is a complete agent experience over supported profiles and geographies, not another collection of independently working primitives.**

## 2. Preserve the ambition; retire only unsupported claims

### 2.1 Candidate ranking still belongs

The historical E3 point-repair hypothesis failed on its measured population. That result must remain recorded. It does not establish that ranking candidates is universally useless. The retained [Cornerstone national-evidence experiment](../scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17/README.md) demonstrates a different, narrower capability: supplied national evidence favors one parcel among 2,983 while all 2,983 hard-feasible parcel models remain; two buildings tie among 347 bounded building candidates.

Preserve three distinct operations:

- **Candidate discovery/reach repair:** put the relevant entity into the bounded universe.
- **Candidate ranking:** explain which available alternatives the evidence favors.
- **Exact reasoning:** establish what follows from admitted constraints, including ambiguity and contradiction.

Ranking cannot recover a truth excluded upstream. Ranking can still be valuable when truth is reachable. An uncalibrated preference is not a probability, and a soft winner is not a forced identity.

The old point-re-ranking product premise remains cut under the governing plan. New bounded point validation/ranking work must be explicitly scoped, measured, and reconciled with that decision; it must not advertise the old experiment as a success.

### 2.2 National economics still belongs

Retire the unsupported per-tile and national cost numbers, not the objective of economical national operation. Restore **measured cost-versus-answer-quality** as a delivery track. Instrument immediately; make national projections only when the measured workload distribution supports them. Section 9 defines the scope.

### 2.3 “This dataset could finish the job” still belongs

Make this a generated, evidence-backed result, with distinct statements for logical sufficiency, dataset availability, and empirical usefulness. A promising dataset is not a guarantee that the needed record exists. Section 8 defines the distinction.

### 2.4 Exactness and usability reinforce each other

The answer must distinguish supported association, acceptance under policy, solver exactness, and complete extent. Hiding useful narrower conclusions behind a global abstention is not extra rigor. Exposing them without inflating their strength is the goal.

These distinctions extend the agent-facing direction already recorded in [the architecture](CANON_GEO_AGENT_ARCHITECTURE.md) and the [physical-asset-resolution proposal](PLAN_CANON_GEO_PHYSICAL_ASSET_RESOLUTION.md).

## 3. Diagnosis: the primitives exist, but the agent still assembles the product

At the reviewed baseline, the seven primary commands, nine-stage default composition plan, deterministic evidence compiler, factorized solver, stored-run inspection, and next-evidence controller provide substantial reusable machinery. See [plan.rs](../src/geo/plan.rs), [run.rs](../src/geo/run.rs), [evidence.rs](../src/geo/evidence.rs), and [composition.rs](../src/geo/composition.rs).

The remaining product gaps are concrete:

| Observed baseline | Product consequence | Required change |
|---|---|---|
| The architecture records a Brooklyn run with a valid ambiguous solve and seven completed stages, followed by failure for missing prospective separation inputs | A useful current answer can be obscured by an unavailable future-work stage | Make current-answer completion an explicit endpoint; report enhancement state separately |
| The next-evidence API consumes caller-supplied candidate actions | The agent must invent what the tool should ask for next | Generate actions from unresolved claims, inventory, and reusable evidence recipes |
| The retained national-evidence example uses experimental adapters and manually assembled inputs | A successful experiment does not yet yield a repeatable ordinary-agent workflow | Package adapters, local bindings, acquisition handoffs, and resume transitions |
| Native address-membership semantics remain NYC/PAD-specific; the general address-first journey remains open | A known address is conceptually accepted but not uniformly executable | Deliver declared, tested address profiles without imposing NYC semantics elsewhere |
| The current default planner is parcel/building-specific | A broader requested answer can exceed shipped semantics | Preserve supported narrower answers and make unsupported grains explicit |

Sources: [architecture §6](CANON_GEO_AGENT_ARCHITECTURE.md#6-minimal-agent-command-surface), [next_evidence.rs](../src/geo/next_evidence.rs), and [Cornerstone retained evidence](../scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17/README.md).

A specific controller gap matters: an empty supplied action list with an ambiguous composition can currently produce `HonestAmbiguity`. No supplied actions does not prove that action discovery was complete. Replace that inference with explicit discovery coverage; preserve genuinely justified stopping conditions. This is a proposed correction, not a claim that it has landed.

## 4. The boundary: responsibilities, not merely network access

**An offline deterministic kernel must not imply a manually operated product.**

| Participant | Owns | Must not have to do / must not do |
|---|---|---|
| **Calling agent or Evidence Machine** | State the subject and desired claim; provide known observations and provenance; set as-of, permissions, policy, and budget; resolve genuine business ambiguities | Must not routinely construct solver constraints, stage bindings, prospective outcome partitions, or ad hoc adapters |
| **Geo inquiry workflow** | Select compatible installed profiles; discover required evidence capabilities; construct bounded candidates; produce acquisition requests; bind returned evidence; advance/replan; expose answers, changes, and next actions | Must not hide network activity inside the offline kernel or invent stronger acceptance |
| **Versioned source adapters and recipes** | Interpret source fields, addresses, units, geometry, temporal scope, lineage, and supported relations; turn retained bytes into typed evidence under declared contracts | Must not special-case the demonstration subject, hard-code source authority globally, or silently promote soft observations |
| **Authorized acquisition executor** | Execute bounded MCP/catalog/source requests; handle transport and pagination; retain returned bytes and receipts; enforce access and spend permissions | Must not decide identity, fabricate missing records, or describe incomplete retrieval as complete coverage |
| **Offline Geo kernel and shared project runner** | Validate, materialize, admit, propagate, solve, explain, retain receipts, and reuse valid work | Must not call the network, own credentials, or become a second scheduler/cache beside `src/project/` |

The workflow is part of the **Geo product experience** even when its acquisition executor runs in an agent harness or an external service. It invokes the existing shared project substrate. Evidence Machine may orchestrate the larger business inquiry; Geo owns the reusable geographic workflow within it.

Two deployment modes should share identical contracts:

1. **Agent-mediated:** Geo returns an executable acquisition packet; the calling agent uses its authorized MCP connection and returns a receipt.
2. **Managed executor:** an authorized integration performs those same packets automatically within the granted budget, pausing only for permissions or meaningful decisions.

Neither mode requires putting credentials or network calls in `geo run`. Novel evidence can still be supplied by an agent; repeatable source translation should become a reusable adapter rather than repeated subject-specific glue.

## 5. One inquiry contract, two entry paths

This section describes proposed interaction semantics, not new shipped commands, flags, or schema versions. Prefer extending the existing primary surface and shared contracts over adding another family of verbs. Pin installed defaults visibly rather than asking the agent to author five configuration files for every subject.

### 5.1 Common inputs

An inquiry carries its identity, mode, requested claim(s) and entity grain(s), spatial scope, evidence-as-of requirements, policy, permissions, budgets, and source-backed observations. Preserve original supplied values, uncertainty, units, meaning, and provenance. Omitted or unknown information is not negative evidence.

Geographic scope must have an explicit interpretation. “Orlando market,” “City of Orlando,” a county, and a bounding box are not interchangeable. An ambiguous place name should produce a bounded disambiguation action, not an invisible scope choice.

### 5.2 A: known-address corroboration

Illustrative intent:

> Given this supplied address and optional property name, show the evidence-supported address interpretation, associated parcels/buildings, competing readings, and the limits of what is established.

The workflow should parse without destroying directionals, units, ranges, or locality distinctions; discover applicable address and physical evidence; construct candidates; test the supplied association; and return claim-specific support. An address-only request must not require a loan, SEC accession, CMBS deal, or complete collateral schedule.

Wrong or ambiguous input is a valid case. Report conflicting evidence and possible corrections with provenance. Do not silently replace the address and present the replacement as confirmation.

### 5.3 B: region-and-attributes discovery

Illustrative intent:

> Within this region, identify the likely address of an apartment property with this name and approximately this unit count; show the candidates and evidence.

The workflow should select bounded candidate-generation strategies from available source capabilities. Attributes may include a name, type, units, area, owner/operator, developer, vintage, or proximity description. A few attributes are valid input; a pre-resolved location is not required.

Return source-backed candidate addresses and physical associations, with deterministic preference ordering where justified, explicit ties, supporting/conflicting evidence, and candidate-reach limits. Unknown attributes must not automatically exclude candidates. Approximate quantities and incompatible measurement semantics must not become exact filters.

Discovery must not reduce to “ask another tool for an address, then start Geo.” External retrieval is allowed, but Geo's packaged workflow owns what to request and how to evaluate what returns.

### 5.4 Shared progression

```text
known address -------------------\
                                  -> inquiry + declared scope/profile/policy/budget
region + qualifying attributes --/                 |
                                                   v
                                  bounded discovery/acquisition packets
                                                   |
                                  retained evidence + validated bindings
                                                   |
                                  candidates -> admission -> reasoning
                                                   |
                                  current answer + evidence + alternatives
                                                   |
                                  next-evidence options or justified stop
                                                   |
                                  new evidence -> new immutable revision
```

If discovery finds an address, corroboration is another stage of the same inquiry. Evidence reused across stages remains the same lineage, not another vote. Different requested grains can complete at different strengths.

## 6. A complete current answer, not a single status label

Expose execution, answer quality, and future-work status separately. These are orthogonal dimensions, not one escalating confidence ladder.

| Dimension | Required information |
|---|---|
| Execution | What completed, failed, awaits input, or hit a budget; where execution stopped |
| Association support | Subject, relationship, candidate, grain, time, support, contradictions, alternatives, and source links |
| Preference | Favored candidates, ties, reasons, sensitivity to evidence, and explicit lack of probability calibration where applicable |
| Acceptance | Whether a named, versioned policy permits the requested use; unmet requirements; no implicit acceptance when policy is absent |
| Logical consequence | Hard-feasible residual, forced members, conflict, count exactness, and representation/budget limitations |
| Extent and reach | Whether the bounded universe can include truth; whether complete property/collateral membership is established |
| Further work | Available actions, discovery gaps, conditional effects, costs, and justified stopping reasons |

Keep provenance reachable in one bounded inspection. The agent should not have to join unrelated JSON files to explain a result. Present human-readable claims from structured fields; narrative must not strengthen them.

**Required lifecycle change:** introduce an explicitly requested current-answer endpoint that can complete after the applicable solve/explain work. Optional acquisition planning must not invalidate it. Do not relabel the previously failed nine-stage baseline as complete. Full-workflow requests must preflight required inputs and continue to report unfinished required stages honestly.

An illustrative rendering, not a measured new result:

> The evidence favors address A and parcel P. Two buildings remain possible. This supports research navigation but does not meet the requested building-identity acceptance policy. Complete property extent is unestablished. Next-action discovery is incomplete because the local building-address source has not been inspected.

The run is useful even before the next action exists. A successful execution is not proof of a correct association, and a useful association is not proof of complete extent.

## 7. Make acquisition and adaptation an executable handoff

An acquisition packet must identify the question and claim it serves, applicable source/adapter version, release or explicit discovery selector, geographic/row/column bounds, ordering and pagination, output contract, permissions, resource ceiling, and resume binding. Where a supported connector is known, include executable connector arguments or a versioned query recipe, not merely “look in the warehouse.”

The executor returns actual result bytes or a declared retained projection, source/release identity, request/query identity when available, completeness and pagination state, row/byte counts, hashes, proof class, and failure details. Missing provenance limits the claim; it must never be invented to satisfy a validator.

The workflow performs canonical binding, validation, adaptation, and explicit replanning. Reuse shared receipts and immutable revisions. Do not change the meaning of `--satisfy`: receipt validation alone is not inventory advancement.

**Add inquiry-scoped evidence ingestion without weakening regional inventory rules.** A lookup covering two candidate buildings can inform this inquiry without claiming that an entire region is available. Preserve its exact subset, release, lineage, and exclusions. Whole-region advancement retains the stronger existing checks in [the acquisition architecture](CANON_GEO_AGENT_ARCHITECTURE.md).

Preserve distinct outcomes: zero rows, timeout, unreadable fields, partial/truncated results, corrupt bytes, permission denied, and conflicting source content. A zero-row lookup is a recorded retrieval outcome, not proof that the physical asset does not exist unless a justified completeness/absence contract supports that inference.

A fresh-process resume must reconstruct the inquiry from its retained state. Cross-release reuse must follow validated dependencies; do not assume today's same-directory resume already provides every target accretion behavior.

## 8. Generate the next evidence and explain its value

The existing [controller](../src/geo/next_evidence.rs) and [architecture §9](CANON_GEO_AGENT_ARCHITECTURE.md#9-choosing-the-next-evidence) provide a starting point. Add action generation, not another optimizer detached from the workflow.

### 8.1 Four jobs

| Unresolved condition | Action generation should seek |
|---|---|
| True candidate may be absent | Better scope/location evidence, missing source coverage, or bounded universe expansion |
| Admitted evidence conflicts | Verification or adjudication of the named conflicting records/contracts |
| Multiple answers remain | An observation that separates those alternatives |
| A preferred answer lacks required acceptance or complete extent | Direct relationships, adequate validation, or explicit membership/completeness evidence appropriate to that claim |

### 8.2 Evidence recipes

A reusable recipe declares applicability, supported grain/relationship, required fields, join and temporal semantics, evidence authority, possible observation outcomes, the contract induced by each outcome, source capability requirements, acquisition steps, cost bounds, and stopping behavior.

The action generator uses the unresolved claim and current inventory to instantiate recipes. It must include no-record, ambiguous-join, partial-coverage, stale-record, and contradictory outcomes, not just the favorable branch. Distinguish possible data outcomes from acquisition execution failures.

Exact counterfactual separation requires an exact residual and sound exhaustive outcome treatment. If branches are not a proven partition, report overlap/unknown effects rather than pretending model counts define probabilities. Rank by the requested claim's consequences, not raw model-count reduction across unrelated grains.

Permit small, explicitly budgeted multi-source recipes. Two records may jointly establish a relationship that neither can establish alone. Begin with bounded two-step combinations and declared stopping rules; do not launch an unconstrained search over all datasets.

### 8.3 Three different promises

- **Conditionally sufficient:** under the stated contract, a complete applicable observation would settle the requested distinction. Name every premise and remaining acquisition risk.
- **Potentially useful:** an available source might supply that observation, but subject coverage, readability, joinability, or semantics remain unverified.
- **Empirically useful:** a separately evaluated population supports a measured resolution or acceptance uplift, with its denominator, error rates, and costs.

Only claim guaranteed separation when every admissible modeled outcome supports it. Otherwise show the success and unresolved branches. Never promise that buying or retrieving a dataset guarantees identity simply because its schema contains a promising field.

Without an applicable evaluated loss model, expose nondominated choices and reasons rather than manufacture a globally optimal action. A policy reference by itself is not a calculation of expected value. Shared source lineage must not receive duplicate information credit.

### 8.4 Stopping and evidence discovery

Distinguish action discovery not attempted, incomplete, exhausted within declared installed capabilities/budget, and stopped by user policy. An empty action list cannot by itself establish irreducible ambiguity.

Support bounded discovery beyond currently installed recipes through a typed external request. Unsupported sources remain visible gaps; no agent should silently author a new hard-admission rule merely to finish a subject.

## 9. Ranking, evaluation, and national economics

### 9.1 Evaluate both entry points separately

Use declared input bundles, independently adjudicated truth, frozen selection rules, and held-out subjects. For discovery, withhold the target address from the input; obtaining address-bearing evidence through the declared workflow is legitimate, but withheld truth must not leak into candidate generation or profile tuning. For corroboration, include correct, wrong, incomplete, and ambiguous supplied addresses.

Report candidate reach separately from conditional ranking accuracy and end-to-end accepted-answer quality. Include top-k results and ties, false accepted associations, justified abstentions, grain errors, traceability, complete-extent errors, acquisition failures, review burden, and cost. No new metric replaces the existing hard-forced or E4/E5/G3 gates.

Ranking and logical consequence remain parallel outputs. A versioned acceptance policy may permit a limited research use without claiming certainty; higher-stakes or complete-extent claims must meet their own evidence requirements. Missing calibration yields explicit limitations, not an invented percentage.

### 9.2 Three cost workloads

| Workload | Measure separately |
|---|---|
| **Cold preparation** | Acquisition/licensing, transfer, retention, normalization, indexing, and evidence-plane preparation for declared coverage |
| **Warm inquiry** | Incremental retrieval, adaptation, candidate construction, reasoning, explanation, and any review per subject |
| **Refresh/accretion** | New evidence or releases, affected components, invalidation, recomputation, and verified reuse |

Instrument these from the first implementation slice. Deterministic counters govern kernel fallback. Observed duration, hardware, monetary charges, and operational spend controls remain explicitly outside semantic answer identity; acquisition spend limits can stop further retrieval without changing the meaning of already retained evidence.

Build projections from measured distributions across geography, evidence tier, candidate density, component size, source mixes, and failure cases. Report cold/warm distinctions, dense tails, cache assumptions, missing coverage, uncertainty, and unmeasured terms. A partial-geography test is not national coverage, and an extrapolation is never labeled measured national performance.

The deliverable is an evidence-tier curve linking coverage, accepted-answer quality, abstention, and cost, plus a documented projection where supported. Unsupported cost terms stay unknown. Do not revive the old 0.5-second/tile or 140-CPU-hour national figures.

## 10. Delivery sequence: runnable slices with explicit exit gates

All work below is **proposed and unchecked**. These are roadmap labels, not fabricated Beads IDs or claims of assigned ownership. During implementation, map them to existing work items before creating duplicates. In particular, reconcile the architecture's existing inspection, run-completion, and source-neutral address work. Code and positive/negative tests ship together.

### S1 — Complete the current answer (P0; start here)

**Deliver:** an explicit current-answer endpoint, orthogonal execution/claim/further-work reporting, and a usable one-inspection projection. Reuse `run.rs`, `inspect.rs`, `plan.rs`, and their existing artifacts; version changes follow repository doctrine.

**Exit gate:** an ambiguous solve with no prospective actions returns its valid answer and evidence, marks next-action discovery incomplete, and remains resumable. A requested full workflow does not falsely report completion. Corrupt or missing required evidence is not hidden.

Start resource instrumentation alongside S1. Do not begin with a solver rewrite.

### S2 — Package the shared inquiry and both entry paths (P0; after S1)

**Deliver:** two explicit intake modes, visibly pinned defaults, retained inquiry identity, compatible profile selection, automatic local artifact binding, and the workflow driver over the existing project substrate. Address corroboration and region/attribute discovery must both work on declared supported profiles.

**Exit gate:** a caller supplies ordinary subject facts rather than internal node bindings. Discovery returns evidenced candidate addresses without a trusted input address. Corroboration challenges an incorrect supplied address. The same evidence and physical entity can flow between modes without duplicate corroboration credit.

Do not create a new command per module. Any facade/contract addition must be reflected consistently in schemas, capabilities, help, and operator descriptions.

### S3 — Close the acquisition loop with reusable adapters (P0; integrates with S2)

**Deliver:** executable bounded acquisition packets; one agent-mediated executor integration; packaged source adapters for both modes; inquiry-scoped evidence ingestion; canonical receipt binding and explicit replan/resume.

**Exit gate:** a fresh agent uses only public interfaces and returned acquisition instructions. No subject-specific Python/Rust, manual JSON surgery, hand-authored constraints, or undocumented repository knowledge is needed. Pagination, zero rows, unavailable permissions, and failed retrieval all preserve a truthful current answer. No narrow lookup is promoted to whole-region availability.

Use the retained national/local evidence family to accelerate integration, but prove transfer on an unseen subject and at least one non-NYC profile before describing the workflow as source-neutral. This integration check is not a substitute for the full E5 gate.

### S4 — Generate actionable next evidence (P1; after S3)

**Deliver:** recipe-driven action generation for the four jobs, conditional effects, bounded two-step bundles, discovery-coverage state, and evidence-change explanations after resume.

**Exit gate:** the workflow requests a concrete useful acquisition without the agent designing the experiment; one test resolves a real distinction, one returns no relevant evidence, and one exposes contradiction. Each produces the correct revised answer. No supplied actions is never treated as proof of exhausted information.

### S5 — Validate ranking and explicit acceptance (P1; evaluation design starts with S2)

**Deliver:** separate preference and consequence views, versioned policy evaluation, evidence ablations, held-out corroboration/discovery results, and scope-specific point-ranking evaluation.

**Exit gate:** report reach, ranking, false acceptance, abstention, grain, and extent separately. Unsupported confidence and source-count inflation fail tests. The historical point-repair result and frozen gates remain unchanged. Useful positive cases are required; a refusal-only implementation does not pass.

### S6 — Publish measured economics and the release proof (P1; instrumentation already running)

**Deliver:** cold/warm/refresh costs, reuse and tail measurements, evidence-tier quality/cost results, and a clearly labeled national projection only where supported.

**Exit gate:** a clean-session agent completes both use cases, acquires additional evidence through the supported boundary, resumes after restart, explains changes, and reports cost without bespoke orchestration. Declare the supported region/source/profile envelope and all unresolved gaps. Retained fixtures, held-out tests, fresh acquisition, and extrapolations retain distinct proof labels.

### First implementation wave

1. Reproduce the documented current-answer/optional-stage failure and add S1's positive and negative tests; reconcile the current-answer endpoint with the controlling architecture.
2. Package one known-address inquiry and one addressless region/attribute inquiry using retained inputs and shared S2 machinery; instrument their work.
3. Replace their manual acquisition/adaptation steps with S3's public handoff, then exercise an unseen subject through that same path.

Finish this vertical slice before broadening the source catalog, adding speculative solver representations, or attempting nationwide acquisition infrastructure. The retained demonstration establishes integration only; it never becomes held-out accuracy evidence.

## 11. Release checklist and non-goals

The agent-complete version is not ready until all of these hold:

- [ ] Both entry points are runnable and documented as first-class workflows.
- [ ] Discovery produces supported candidate addresses/physical entities without a trusted supplied address.
- [ ] Corroboration can confirm, challenge, or leave a supplied address unresolved with evidence.
- [ ] A useful current answer survives unavailable optional next-evidence work.
- [ ] A clean agent needs no bespoke per-case orchestration for supported profiles.
- [ ] Acquisition requests are executable, bounded, permission-aware, and receipt-bound.
- [ ] Narrow evidence, regional coverage, and proof classes remain distinct.
- [ ] Preference, acceptance, exactness, reach, and complete extent are reported separately.
- [ ] Geo generates a useful next action and explains both successful and unsuccessful outcomes.
- [ ] Fresh-process resume and evidence-change explanation work without losing provenance.
- [ ] Held-out evaluation reports positive capability, wrong answers, abstentions, and failure cases for both modes.
- [ ] Cost and reuse are measured; national projections are labeled and bounded by actual coverage.
- [ ] Existing frozen gates, exact runtime lookup, and review-gated promotion remain intact.

Non-goals for this milestone: a second scheduler; network acquisition inside the deterministic kernel; one generic score pretending to prove identity; a claim that every site has one address; complete collateral reconstruction as a prerequisite for useful association; nationwide crawling before a supported end-to-end slice; or new solver machinery without measured need.

**Bottom line:** preserve ranking, exact reasoning, evidence-aware next actions, and national economics. Make known-address corroboration and region-plus-attributes discovery two first-class paths through one product. Agents should supply facts, permissions, and goals—not assemble the geographic reasoning system around the tool.
