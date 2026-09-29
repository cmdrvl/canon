# Response to the data-center brief: recover the inquiry, not just the solve

**Date:** 2026-09-28.  
**Status:** operator-requested response and executable recovery protocol; documentation only. No recovery experiment, implementation, new acceptance policy, or release gate is claimed complete.  
**Responds to:** [BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md](BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md).  
**Reviewed repository baseline:** `2534b98edd8bcba940219a08ccf3bd29af9230ad`; the brief's blob is `a44cb95b65b5fc34d3266d61cf91f9acc2d7fcd0`. Freeze the actual implementation revision again when executing this protocol.  
**Related work:** `bd-dzdy1` is the original field-test bead in cmdrvl-curves. The existing Geo inquiry roadmap and its beads remain the implementation map; `bd-179b` and the established release gates remain open unless independently satisfied. This document closes none of them.

## 1. Decision in one screen

**Run one controlled recovery experiment before extending acquisition, scaling the solver, or completing the whole inquiry roadmap.**

The question is:

> Given a frozen neighborhood and the same evidence, can we produce a defensible site association, and identify exactly where the current tool loses that ability?

The central diagnosis is not simply that the agent needs better instructions or that the warehouse needs more data:

> The tested path asks evidence to distinguish competing site explanations, but translates much of that evidence into rewards for including individual buildings. With unrestricted set selection, those rewards favor accumulating members rather than distinguishing explanations.

Preserve the exact kernel, evidence provenance, replay, explicit ambiguity, and review boundary. Do not equate those capabilities with a working locator. Do not pivot to claim auditing merely because auditing is the part that currently works.

The immediate implementation unit is **one diagnostic harness, one reviewed end-to-end development case, and the smallest patch justified by the trace**, followed by untouched cases and a fresh-agent replay. A successful result must be supported for the requested claim, not merely smaller or labeled `resolved`.

This is a narrowing of execution, not a replacement architecture. The existing [agent-complete inquiry roadmap](PLAN_CANON_GEO_AGENT_COMPLETE_INQUIRY.md), especially sections 2–6, already separates discovery, ranking, logical consequence, acceptance, and extent. It also says the calling agent must not routinely author solver constraints or assemble pipeline bindings. Make that behavior executable in one bounded case first.

## 2. What is established, and what remains a hypothesis

### 2.1 Confirmed formulation defect: inclusion-only costs favor supersets

The [composition implementation](../src/geo/composition.rs) defines `GeoSoftPreference` with `member` and an unsigned `cost_if_absent`. The cost affects presentation order, not hard feasibility. The retained [Pryor run](geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d/RESULT.md) has ten building candidates, no hard pruning, and positive per-member preferences from three channels.

For selected members `S`, its objective is:

```text
C(S) = sum of w_j for preferences whose member m_j is absent from S
w_j >= 0
```

If `S` is a subset of `T`, then `C(T) <= C(S)`. Therefore, whenever the whole candidate universe is feasible, it is an optimum. If every candidate has positive support, the whole universe is the unique optimum. If only some candidates have support, all feasible supersets of that support union can tie at zero.

The retained Pryor result is exactly the unique-optimum case: all ten buildings, cost zero, among 1,023 nonempty combinations. This is a property of the objective and request, not an inference from a large accuracy sample. More positive weights cannot fix it. More positive membership evidence can reinforce it. Hard constraints can forbid the full set, but that does not make this ranking objective a general site discriminator.

**Boundary:** this does not establish that the exact solver is incorrect, or that positive preferences are useless for all profiles. Mutually exclusive single-candidate alternatives can be ranked by positive evidence. The defect arises when that mechanism is used as a campus-selection objective without adequate competing-hypothesis semantics.

Appendix A gives an independent, standard-library calculation of this property from the retained compilation. Its output is not a fresh warehouse run or a campus-accuracy result.

### 2.2 Alternatives and members are different variables

Suppose observation one permits candidate site A or B, and observation two permits B or C. If these are alternative identities of the same target, their intersection may select B.

If instead A, B, and C are possible members of an assemblage, a composition containing A and C can satisfy both existential observations. Including all three can satisfy everything. An existential statement about alternative identities must not silently become a collection of positive membership claims.

The kernel already has distinct operators such as `AnyOf`, `AllOf`, `AllowedSets`, and cardinality. The [evidence compiler](../src/geo/evidence.rs) also distinguishes observation kinds. The issue is whether the chosen representation and translation preserve the proposition being investigated.

Separate these questions:

- Which site does this evidence identify, at the declared time?
- Which parcels or buildings may belong to that site?
- Is the complete extent established?

They can inform each other without requiring all three to be settled together. Do not repair the experiment by assuming that every campus has exactly one building. Conversely, do not require complete campus membership to expose a supported location association.

### 2.3 Evidence reliability is not evidence discrimination

The [Pryor script](geo_datacenter_site_test_2026-09-28/scripts/run_arm_d_pryor.py) constructs three channels before Canon solves anything: relative proximity to named businesses, repeated building areas, and cross-layer footprint agreement. The compiled request reduces them to preferred members.

| Observation | Directly supports | Does not establish by itself |
|---|---|---|
| Two footprint sources agree | Consistent representation of a physical building | Membership in the target campus |
| Several roofs have similar areas | A morphological pattern | Common operator, ownership, or project identity |
| A roof is closer to one business point than another | A proximity relation | A site boundary or occupancy relation |
| A parcel record names an owner | That source's ownership assertion at its vintage | That all similarly owned parcels form this campus |
| A report mentions acreage | An asserted quantity attached to some described subject | Whether it measures the whole campus, one phase, a purchase, or another extent |

These observations may become useful features under a tested interpretation. Their identity relevance cannot be assumed from source reliability alone. In particular, footprint agreement must not automatically earn an independent vote for operator identity.

Preserve the original statement, subject, relation, measure, uncertainty, valid time, and lineage through translation. The brief reports that the Bexar extraction kept acreage while dropping the operator and park name. That is an evidence-translation failure before any solver issue.

**Working hypothesis:** a substantial part of the missing capability lies in translating heterogeneous observations into discriminating site hypotheses. It is not yet established that a better translation of the currently retained data is sufficient to resolve any particular campus. The recovery experiment must test that, not assume it.

### 2.4 Soft disagreement is not hard exclusion

Keep the rule that uncalibrated evidence cannot silently remove feasible worlds. Do not interpret it as a ban on ranking an interpretation lower when relevant evidence fits it poorly.

Keep three outputs distinct:

```text
logically excluded or forced under admitted assumptions
preferred under a declared, possibly uncalibrated ranking rule
accepted for a named use under a justified policy
```

No calibrated probability is required to test a transparent deterministic heuristic, but an uncalibrated score must not be displayed as a probability. No arbitrary extra-member penalty is justified merely because it makes the output smaller.

Calibration is not the first recovery step. Before making a size band hard, establish that the compared quantities describe compatible measures and extents. A wrong band can exclude truth while leaving a false candidate feasible; it does not necessarily produce an empty set. Likewise, a conflict proves incompatibility of the encoded request, not automatically a defect in a vendor record. Universe construction, interpretation, temporal mismatch, and authored assumptions remain possible causes.

### 2.5 The agent is currently supplying part of the missing model

The field-test author chose radii, minimum areas, neighboring-business interpretations, module thresholds, matching tolerances, and weights. These are inference-model decisions, not just command invocation.

The [inquiry roadmap](PLAN_CANON_GEO_AGENT_COMPLETE_INQUIRY.md) explicitly records manually assembled inputs and caller-supplied next-evidence actions. The [agent architecture](CANON_GEO_AGENT_ARCHITECTURE.md) distinguishes its normative target from shipped behavior.

Treat two failures separately:

1. **Modeling:** the evidence has no justified, discriminating interpretation or cannot be expressed by the selected formulation.
2. **Workflow:** a validated interpretation exists, but an ordinary agent cannot execute it through the supported surface.

Fixing workflow alone can automate the modeling failure. A source adapter may contain necessary jurisdiction-specific semantics; that is not a reason to branch on provider or demonstration identities inside the generic kernel.

## 3. Did the experiments still use the surrounding neighborhood?

**Yes, but in a restricted form. They did not merely examine one point, and they did not test the complete surrounding-scene inquiry.**

The [A–C script](geo_datacenter_site_test_2026-09-28/scripts/run_arms.py) explicitly:

1. Obtains an anchor from an accepted geocoder match or a Foursquare address match.
2. Uses `H3_LATLNG_TO_CELL(..., 7)` and `H3_GRID_DISK(..., 2)` as a spatial prefilter.
3. Applies an additional 1,500-meter centroid-distance condition.
4. Retrieves buildings at least 100,000 square feet, ordered by size, with `LIMIT 40` per layer, from Microsoft, Overture, and FEMA.
5. Reduces the query results to layer-level counts, total area, largest area, and nearest distance.

The returned Census tract is metadata; it is not the search boundary in those building queries. H3 is indexing/blocking, not evidence that the point or a candidate is correct, and a ring count is not a substitute for checking coverage of the declared geometry.

The sweep does not run a full composition inquiry for all 69 sites. Without a usable anchor it reports city-level/unresolved rather than searching the declared region for sites. An accepted geocoder anchor remains the building-search center even when a Foursquare address match is also available. Name-only matches are retained as candidates, not promoted to anchors.

Pryor then uses a ten-building universe: Overture centroids within 1,300 meters and area at least 10,000 square meters. Its script has already computed the three channels before `materialize-evidence -> compile-evidence -> solve`.

Thus the tested progression is:

```text
anchor -> selective nearby retrieval -> hand-designed membership signals -> set solve
```

The intended progression is closer to:

```text
question + uncertain location/region -> bounded surrounding evidence
-> interpretable candidate hypotheses and relations
-> supported association + uncertainty + next useful evidence
```

The difference matters. The field test does not demonstrate that a complete neighborhood-reasoning implementation was tried and failed. It does demonstrate defects in the tested retrieval and translation path. Reintroducing more neighborhood rows without fixing the formulation is not sufficient.

## 4. Corrections to the brief's diagnosis

| Brief emphasis | Response |
|---|---|
| H1: reach dominates | Reach fails in the tested retrieval routes, but it cannot explain the inclusion-only objective when candidates are present. Measure the two separately. An absent anchor is not proof that no usable regional evidence exists. |
| H2: calibrate a hard exclusion channel | First preserve evidence meaning and demonstrate discrimination on fixed evidence. Calibration may later justify a policy or band; do not promote approximate observations to hard solely to obtain a singleton. |
| H3/H8: assemblages and combinatorial scale | Real assemblages matter. Do not turn location identification into unrestricted subset enumeration by default, or start by scaling that enumeration. Establish which claim and hypothesis space are needed. |
| H4/H5: heterogeneous parcel planes and NYC transfer | Real risks. Keep the initial diagnostic corpus fixed; use untouched cases to expose transfer limits after the first repair. County adapters remain necessary where semantics differ. |
| H6: perhaps the product is auditing | Do not use an implementation failure as evidence for a commercial pivot. Test the original association promise first. |
| H7: authored hard requests bypass evidence admission | Preserve authored assumptions for experiments, label them clearly, and never treat their success or conflict as calibrated acceptance. The recovery path must not use that bypass to manufacture resolution. |
| H9: truth gap | Label the requested association and time independently. A facility point cannot establish complete building membership. |
| H10: pipeline drops useful evidence | Treat this as a first-class candidate root cause, not an incidental upstream inconvenience. |

The headline that the nearest-point singleton ranked 1,019th must remain a **ranking observation**, not be restated as the known correct campus ranking 1,019th. The retained [run caveats](geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d/RESULT.md) explicitly say that the facility point cannot score building-level membership.

The strongest current conclusion is narrower and stronger than a general accuracy claim: **the Pryor objective has a demonstrable superset bias, while the complete association workflow has not been validated by this experiment.**

## 5. Recovery protocol: execute in order

Do not create a broad new subsystem or a separate orchestration stack. Use the existing measurement area and shared project runner. Proposed artifact names below are a specification for the experiment, not files or capabilities shipped by this documentation change.

### Step 0 — Pin and reproduce the failure before changing it

**Responsible:** implementation owner.

1. Read `AGENTS.md`, the brief, its [evidence README](geo_datacenter_site_test_2026-09-28/README.md), and this response.
2. Inspect the current inquiry beads before claiming or creating work. Attach this experiment to the appropriate existing work; do not mark S1–S6 or the release gate complete because this document exists.
3. Freeze the repository revision, binary version, input digest, budgets, and environment. Use a separate worktree; do not disturb another agent's working tree.
4. Run the independent objective check and the retained Pryor solve from Appendix A. Keep stdout, stderr, exit code, and digests.
5. If current output differs from the historical manifest, retain both identities and explain the semantic difference. Do not regenerate the historical golden to hide a change.

**Output:** an immutable baseline record and the known superset regression. This is solver/formulation evidence, not site accuracy.

**Gate:** the current behavior is reproducible or its drift is explained. If it is not, diagnose that first. No weight changes yet.

### Step 1 — Freeze the question, cases, and independent evaluation

**Responsible:** implementation owner plus a separate adjudicator/reviewer where available.

Use one development case and three untouched real cases. This is a diagnostic suite, not a representative accuracy sample.

- Development: an already-landed neighborhood with independently checkable site association and meaningful alternatives. Prefer one with usable parcel/place/address relations; do not select merely because nearest-point scoring is convenient.
- Holdout 1: another checkable known-address/rough-point case.
- Holdout 2: a checkable region-and-attributes case without a supplied trusted point. At least one positive case should involve a multi-building or multi-parcel site.
- Holdout 3: a genuinely ambiguous or insufficient-evidence case whose uncertainty can be adjudicated.

Select and record the cases before exposing evaluation labels or tuning. If an eligible case cannot be independently labeled, record that gap rather than manufacture a label. Failure on addressless discovery remains an open product limitation, even if the anchored path improves.

For each case, freeze the exact question, target relationship, requested grain, as-of date, original observations, permissible sources, spatial-scope meaning, and resource budget. The initial question is **site association**, not complete campus membership. Score membership/extent only where separately requested and independently labeled.

The adjudicator retains an evaluation-only answer artifact containing sources, lineage, uncertainty, acceptable alternatives, and supported grain. The case analyst and fresh agent must not receive it. Do not use the evaluation point to center retrieval. Do not count an operator address both as supplied evidence and as independent corroborating truth. When source overlap cannot be avoided, disclose it and do not claim independent validation of that relation.

Pryor remains a dedicated formulation regression. It becomes a location/extent accuracy case only to the degree an appropriate independent label is obtained.

**Output:** `case_manifest.json` plus an isolated evaluation label artifact or reference. These are proposed experiment artifacts.

**Gate:** the question and the evaluation measure refer to the same object, grain, and time; the analyst has not seen the answer key.

### Step 2 — Freeze the raw neighborhood once, preserving meaning

**Responsible:** acquisition owner, using authorized read-only source access.

1. Begin with the declared uncertain anchor(s) or bounded region, not a truth label. Preserve competing anchors and their provenance rather than silently selecting one as correct.
2. Use H3 to retrieve a covering set, then check the actual spatial predicate against the declared geometry. Record resolution, rings/coverage, distance policy, CRS, and transformations. A point near a cell boundary must not cause a silent coverage hole.
3. Retain relevant available buildings, parcels, place/address records, ownership observations, and original subject evidence. Retain smaller objects and context needed to test associations; do not inherit the 100,000-square-foot filter or largest-40 cap without justification.
4. Record per-source keys, release/vintage, geometry, original values, units and measurement meaning, source lineage, and exact query bounds. Deduplicate source representations without collapsing distinct physical entities.
5. Validate pagination, returned-row counts, and declared completeness. If a bound or source contract prevents full retrieval, expose a gap or continuation; do not label a capped result complete. Honor the source's query contract rather than removing safety restrictions.
6. Choose centroid, intersection, or containment semantics deliberately. A large parcel can intersect the neighborhood while its centroid lies outside it; a footprint can cross a tile boundary. Record what is retained and what the policy could miss.
7. Store raw observations or permitted lossless projections and receipts, not just derived preference flags. Preserve operator/project/place names and the subject of each quantity. Mark absent parcel or permit evidence as unavailable rather than inventing it.
8. Freeze and hash this snapshot. All compared interpretation/inference paths must use the same bytes. A later acquisition creates a new snapshot and a separately reported experiment revision.

**Storage:** keep private or redistribution-restricted rows in the approved evidence store. Commit safe small fixtures and manifests only. The original evidence README deliberately excludes certain raw records and personal details; do not reverse that treatment casually.

**Output:** a snapshot manifest, retained observations/reference bytes, and explicit availability/coverage limits. Place the safe experiment record under `scripts/geo_measurements/fixtures/datacenter_recovery_2026-09-28/` when implementation starts; that directory is proposed here.

**Gate:** the source estate does not need to be globally complete, but the declared retrieval and its known gaps must be inspectable. For the diagnostic development comparison, the evaluator must be able to determine whether the relevant answer is representable without disclosing it to the analyst.

### Step 3 — Build an evidence-to-answer trace before selecting a patch

**Responsible:** reviewer working only from the frozen snapshot, without new searches or the evaluation key.

Write down the plausible site interpretations and the observations that distinguish them. A hypothesis must identify the subject relationship, time, and candidate site/association. Known members, possible members, and unknown extent stay separate. Do not enumerate every arbitrary subset merely because the kernel can do so; do not hand-remove plausible alternatives to make a favored hypothesis unique.

For each consequential observation, record:

```text
source record and retained text/geometry
-> interpreted proposition: subject, relation, object/value, grain, time
-> candidate(s) affected and shared lineage
-> effect: supports / contradicts / compatible but nondiscriminating / unknown
-> current adapter/compiler representation
-> current computational effect
-> why any proposed different effect is warranted
```

Require statements such as: “This favors A over B because ...”, not just “this is a good source.” Unknown is not disagreement. A reported building's existence is not campus membership. A name alias, owner/operator link, or acreage-scope interpretation needs its own evidence or explicit hypothesis.

Keep a possible **answer absent from this inventory** condition. A bounded list is not proof that it includes the true answer. If hypothesizing groups or associations changes the candidate universe, track reach at that stage too; do not hide truth exclusion behind better ranking.

Then run the current path on the same snapshot and locate the first lost distinction:

| First broken link | Action |
|---|---|
| Relevant source or candidate did not enter the snapshot | Repair one retrieval/coverage defect and freeze a new snapshot. Rerun both paths; do not attribute this improvement to inference. |
| Raw evidence arrived but the subject, relation, units, or time were lost | Patch extraction or the versioned adapter; retain a before/after proposition trace. |
| Correct distinctions are represented, but ranking ignores/reverses them | Patch the formulation, with positive and negative tests using the same inputs. |
| Reviewed configuration works, ordinary agent cannot execute it | Package the validated path through the existing workflow. |
| The frozen evidence supports several explanations equally | Retain ambiguity and specify the missing observation. Do not tune toward the answer key. |

A reviewer finding an answer is not itself ground truth or a guarantee that it can be automated. Check the reviewed interpretation against isolated evaluation, then encode the successful reasoning explicitly. Treat explanations that require undeclared outside knowledge as missing evidence.

**Output:** one trace and a named root-cause decision. Reuse a simple structured table/JSON plus the experiment README; do not create a speculative reporting subsystem.

**Gate:** a proposed patch has a particular failed distinction to restore. If the evidence does not distinguish, perform at most the next justified acquisition within budget and repeat with a new snapshot. Otherwise stop with the measured gap.

### Step 4 — Implement the smallest evidence-preserving change

**Responsible:** implementation owner.

Keep the old path available as the experimental baseline. Compare it with the corrected path on identical retained input. Preserve a simple lookup/nearest-candidate baseline where applicable, clearly labeled with its actual behavior and requested grain.

If the failure is formulation, the minimum requirement is to compare competing interpretations using meaningful support and disagreement while retaining uncertain possibilities. The implementation technique follows the trace; this document does not pre-authorize a probabilistic engine, a new solver, or a universal scoring formula.

Rules for the patch:

- Do not replace superset bias with an arbitrary singleton rule or cardinality penalty.
- Do not promote uncalibrated evidence to hard to force a winner. An authored hard assumption stays an assumption.
- Do not claim one generic score works across ownership, occupancy, footprint existence, location, and complete extent without separately declared semantics.
- Do not turn source count into independent support. Shared provenance must be visible; distinct lineage labels alone do not establish independence.
- Preserve generic core dispatch by evidence/relationship type. Source and jurisdiction semantics belong in reusable versioned adapters/profiles, not branches on site names, hashes, or IDs.
- Keep the hard-feasible result intact when adding a soft disagreement or preference. Expose any material sensitivity to plausible parameter changes; do not hide a tie behind deterministic tie-breaking.
- Reconcile any actual contract change with the governing plans explicitly. This response is not an override of frozen decisions or permission to weaken existing acceptance gates.

**Output:** the minimal code change and its tests in one implementation unit, with a semantic before/after diff. Existing registry replay remains unchanged.

**Gate:** improvement is traceable to evidence meaning, not to fewer output members, altered truth, weakened tests, or leaked labels.

### Step 5 — Run behavioral controls before more field work

These controls test observed failure classes, not an exhaustive new conformance program. Identify them as synthetic/mutated tests rather than live evidence.

| Control | Required behavior |
|---|---|
| Add a known-unrelated nearby building | Its mere inclusion must not improve the proposed site's evidential support or accepted extent. It may legitimately affect inventory/ambiguity reporting. |
| Duplicate an observation with the same upstream lineage | Provenance may grow; independent identity support must not. Do not collapse genuinely new independent evidence under this control. |
| Add a relevant but uncertain contradiction | Show and evaluate the disagreement without automatically making the candidate logically impossible. |
| Add an observation that explicitly distinguishes the competing identities in a controlled fixture | Preference must move for the stated reason, or report a representation gap; a constant union result fails. |
| Replace an attribute with unknown | Do not turn missingness into a known mismatch. Loss of positive support may still change preference. |
| Provide site-location evidence but incomplete extent evidence | Expose the supported association and unresolved boundary separately. |
| Deliberately omit the target from the inventory | Do not promote the best remaining candidate to certainty or acceptance solely because it ranks first. A low-ranked/soft wrong candidate may remain visible if the inventory gap and non-acceptance are explicit. |
| Shift the anchor within its declared uncertainty or across an H3 boundary | Preserve coverage under the declared policy, or disclose why expansion/reacquisition is needed. Do not silently exclude the previously supported site. |

The patched path need not make every uncertain case resolve. It must distinguish useful support, honest ambiguity, known gaps, and unsupported claims. Preserve the historical Pryor fixture rather than rewriting it to make new semantics look backward-identical.

**Gate:** no semantic regression in these controls; all failures remain recorded. Run relevant Rust tests and the repository's standard gates before committing code.

### Step 6 — Make a fresh agent execute the validated path

**Responsible:** workflow owner or the same implementation owner in a fresh session.

Give a fresh agent only the original question, declared observations, authorized evidence access, supported profile, and budget. Hide evaluation labels, the reviewed candidate choice, bespoke weights, and the analyst's completed trace. Retain the agent prompt, model/tool versions, actions, errors, and interventions.

Use the existing primary surface: `geo capabilities`, `geo plan`, `geo run`, `geo inspect`, and the existing acquisition/resume path as applicable. Leaf commands in Appendix A are for diagnosis, not proof of a complete primary workflow. Do not add a new command per subproblem or a second scheduler/cache/receipt store.

The agent must not invent a case-specific solver request. Repeatable acquisition and semantic translation should be packaged in the existing workflow and adapters. An executable acquisition handoff is compatible with keeping credentials/network access outside the deterministic kernel.

Expose a current answer even when optional next-evidence work is unavailable. Do not relabel an incomplete required workflow as complete. A missing next-action list must not become a claim that no useful action exists.

**Gate:** the same material association, uncertainty, and evidence trail are reproducible without case-specific engineering. Count human interventions explicitly. A reviewed manual configuration is not a pass for the agent workflow.

### Step 7 — Freeze the patch and run the untouched cases

No tuning on holdout outcomes. A code or policy change after seeing them creates a new development iteration; disclose the contamination and obtain fresh holdouts for the next transfer check.

Record per case and per path:

| Measure | Required interpretation |
|---|---|
| Availability and retrieval coverage | Which sources and declared bounds were actually inspected, with gaps |
| Candidate/hypothesis reach | Whether evaluation shows the answer was representable; separate candidate loss from ranking loss |
| Association correctness | Correct, incorrect, ambiguous, or unsupported at the requested grain and time |
| Preference | Rank or tie set, reasons, and sensitivity; not a calibrated probability unless independently justified |
| Acceptance | Named policy and met/unmet conditions; a soft winner is not automatic acceptance |
| Extent | Known/possible members and whether completeness is established |
| Countermetrics | False inclusions/exclusions, grain errors, unsupported certainty, and unresolved cases |
| Agent usability and cost | Tool calls, bytes/rows, run time, resource budget, and human interventions |

Compare three paths: current Canon, the reviewed corrected formulation, and the fresh-agent packaged path. Include the simple baseline where applicable. They must share retained evidence for an inference comparison; report acquisition changes separately.

A bounded recovery can be credited only if it produces an independently checked positive association, preserves the negative/ambiguous controls, and transfers to the untouched positive cases without custom logic. If it succeeds only for anchored cases, report that boundary and keep region discovery open. If it only matches the simple baseline, state the demonstrated provenance/usability benefit separately; do not claim an accuracy lift.

Zero observed wrong acceptances in this small suite is not a population error rate. These cases do not satisfy E4/E5, the truth-rebuild gate, national scale, or commercial readiness by themselves.

## 6. Minimal deliverables and stop decisions

The implementation follow-up should leave only what is needed to inspect and repeat the result:

1. One safe fixture directory with the case/snapshot manifest, permitted observations or immutable references, and isolated evaluation linkage.
2. One replay/comparison harness and the positive/negative tests appropriate to the actual patch, using existing measurement conventions.
3. One results record with baseline/corrected/fresh-agent outputs, semantic diffs, costs/interventions, failures, and the root-cause decision.
4. The smallest justified code/adapter/workflow change, linked to the existing inquiry work and accompanied by required tests.

Do not begin by landing more states, fitting a broad calibration model, scaling unrestricted set enumeration, adding many new source adapters, rewriting the solver, inventing a new CLI family, or finishing all roadmap slices. A specific missing source can be acquired after the trace identifies its role; that is different from expanding the estate speculatively.

Stop with an actionable result rather than manufacturing success:

- **Evidence insufficient:** name the missing proposition and a source/action that might establish it, with coverage and budget limits. Do not assert that the source will contain the needed record.
- **Translation/formulation repaired, agent path still broken:** retain the successful evidence trace and scope the next workflow patch.
- **Anchored path works, addressless path fails:** record a partial recovery and the unimplemented discovery behavior.
- **No benefit over the simple baseline:** retain the comparison and decide whether the extra reasoning serves a distinct claim; do not hide the result behind exactness metrics.
- **All bounded checks pass:** report recovery only for the tested profiles and conditions, then resume the existing roadmap using the remaining measured gaps.

**Acceptance statement:**

> The evidence makes the correct site interpretation preferable for inspectable reasons; unsupported extent stays uncertain; negative cases do not become false acceptance; and a fresh agent can reproduce the result without case-specific engineering.

## Appendix A — Exact baseline commands available now

These commands reproduce and inspect retained inputs. They do not execute the proposed recovery workflow. They require a local clone, Git, the repository's Rust toolchain, and Python 3. The documentation change containing this appendix does not claim that these commands or the recovery gates have been run.

Run from an existing Canon clone. Use an empty separate worktree so other agents' changes are not touched. The reviewed SHA above is a historical reference; `BASE` below records the actual remote revision selected for this run.

```bash
set -euo pipefail

git fetch origin
BASE=$(git rev-parse origin/main)
WORKTREE=$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-recovery.XXXXXX")
git worktree add --detach "$WORKTREE" "$BASE"
cd "$WORKTREE"

export EVIDENCE
EVIDENCE=$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-recovery-evidence.XXXXXX")
printf '%s\n' "$BASE" > "$EVIDENCE/repository_revision.txt"
git status --short > "$EVIDENCE/worktree_status.txt"
rustc --version > "$EVIDENCE/rust_version.txt"
cargo --version > "$EVIDENCE/cargo_version.txt"
python3 --version > "$EVIDENCE/python_version.txt"
cargo run --quiet --locked --bin canon -- --version \
  > "$EVIDENCE/canon_version.txt"

TEST=docs/geo_datacenter_site_test_2026-09-28
INPUT="$TEST/runs/02_pryor_arm_d/pryor.evidence_compilation.json"
```

### A1. Reconstruct the objective without Canon or the warehouse

This intentionally checks only the retained all-building, no-hard-constraint request. It must refuse a materially different request rather than pretend to be a general replacement solver.

```bash
python3 - "$INPUT" <<'PY' | tee "$EVIDENCE/pryor_objective_check.json"
import hashlib
import itertools
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
raw = path.read_bytes()
artifact = json.loads(raw)
req = artifact["composition_request"]
universe = req["universe"]
if universe["parcels"] or req.get("hard_constraints"):
    raise SystemExit("This diagnostic requires no parcels and no hard constraints")

ids = sorted(b["id"] for b in universe["buildings"])
prefs = req.get("soft_preferences", [])
if len(ids) != 10 or len(set(ids)) != len(ids):
    raise SystemExit("Unexpected retained Pryor universe; inspect before proceeding")

support = {member: 0 for member in ids}
for pref in prefs:
    member = pref["member"]
    weight = pref["cost_if_absent"]
    if (member["level"] != "building" or member["id"] not in support
            or type(weight) is not int or weight < 0):
        raise SystemExit("Unexpected preference; inspect before proceeding")
    support[member["id"]] += weight

models = []
for size in range(1, len(ids) + 1):
    for chosen in itertools.combinations(ids, size):
        selected = set(chosen)
        cost = sum(w for member, w in support.items() if member not in selected)
        models.append((cost, chosen))
minimum = min(cost for cost, _ in models)
best = [members for cost, members in models if cost == minimum]

if len(models) != 1023 or minimum != 0 or best != [tuple(ids)]:
    raise SystemExit("Historical superset signature changed; retain and investigate")

print(json.dumps({
    "input_sha256": hashlib.sha256(raw).hexdigest(),
    "candidates": len(ids),
    "preferences": len(prefs),
    "nonempty_models": len(models),
    "minimum_cost": minimum,
    "optimal_models": len(best),
    "full_universe_is_unique_optimum": True,
    "meaning": "Objective reconstruction, not a site-accuracy measurement"
}, indent=2))
PY
```

### A2. Replay the current binary and compare the historical manifest

```bash
set +e
cargo run --quiet --locked --bin canon -- geo solve --request "$INPUT" \
  > "$EVIDENCE/pryor.solve.json" 2> "$EVIDENCE/pryor.solve.stderr"
STATUS=$?
set -e
printf '%s\n' "$STATUS" > "$EVIDENCE/pryor.solve.exit_code"

python3 - "$EVIDENCE" "$TEST/data/solve_manifest.json" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
manifest = json.loads(Path(sys.argv[2]).read_text())
expected = next(x for x in manifest if x["file"] == "02_pryor_arm_d/pryor.solve.json")
raw = (out / "pryor.solve.json").read_bytes()
status = int((out / "pryor.solve.exit_code").read_text().strip())
actual_hash = hashlib.sha256(raw).hexdigest()
report = {
    "exit_code": status,
    "actual_bytes": len(raw),
    "actual_sha256": actual_hash,
    "historical_sha256": expected["sha256"],
    "historical_bytes": expected["bytes"],
    "historical_byte_match": actual_hash == expected["sha256"],
}

try:
    solve = json.loads(raw)
    ranked = solve.get("soft_ranked", [])
    report["status"] = solve.get("status")
    report["residual_model_count"] = solve.get("summary", {}).get("residual_model_count")
    report["first_ranked_model"] = ranked[0] if ranked else None
except (ValueError, AttributeError):
    report["parse_error"] = "Output is not the expected solve object; inspect stderr"

(out / "pryor.baseline_report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
if status != 0:
    raise SystemExit("Solve did not complete normally; inspect retained exit code and stderr")
if not report["historical_byte_match"]:
    raise SystemExit("Output drift: preserve historical inputs and review before proceeding")
PY
```

The expected historical digest is sourced from the retained [solve manifest](geo_datacenter_site_test_2026-09-28/data/solve_manifest.json), not generated from the new run. A difference is a diagnostic stop, not proof of an inference regression: serializer/schema or semantic changes must be separated. Do not overwrite the old manifest.

### A3. Implement and verify only after the trace identifies a change

Create a normal implementation branch in the separate worktree before modifying code. Reuse or claim the appropriate existing bead. Do not use an unrecognized new command or a fictional end-to-end recovery script: the new harness is a deliverable of Steps 3–6, not a pre-existing capability.

For a substantive implementation change, run the repository's gates:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Record the exact fixture/harness command that exercises the new behavior, including fresh-process replay and the primary agent workflow. A unit test, a leaf solve, and a complete inquiry are different checks. Commit only the reviewed implementation scope, safe evidence, tests, and authorized task-state changes; never delete or overwrite another agent's work. Push the implementation branch and verify its remote commit. The present document-only commit is not that implementation completion.

## Appendix B — Implementation reading map

Read these files for the specific failure found; do not edit all of them by default.

| Concern | Existing source |
|---|---|
| Original observations, limits, retained evidence | [Brief](BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md), [evidence README](geo_datacenter_site_test_2026-09-28/README.md), [Pryor result](geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d/RESULT.md) |
| H3/radius retrieval, anchor policy, row limits | [run_arms.py](geo_datacenter_site_test_2026-09-28/scripts/run_arms.py) |
| Hand-designed evidence translation | [run_arm_d_pryor.py](geo_datacenter_site_test_2026-09-28/scripts/run_arm_d_pryor.py), [retained Pryor input](geo_datacenter_site_test_2026-09-28/runs/02_pryor_arm_d/pryor.evidence_compilation.json) |
| Feasibility and membership preferences | [composition.rs](../src/geo/composition.rs) |
| Observation meaning and admission | [evidence.rs](../src/geo/evidence.rs), [materialize.rs](../src/geo/materialize.rs) |
| Inquiry planning and execution | [plan.rs](../src/geo/plan.rs), [run.rs](../src/geo/run.rs), [shared project substrate](../src/project/) |
| Current-answer explanation and inspection | [explain.rs](../src/geo/explain.rs), [inspect.rs](../src/geo/inspect.rs) |
| Candidate next actions versus action discovery | [next_evidence.rs](../src/geo/next_evidence.rs) |
| Governing product/implementation boundaries | [AGENTS.md](../AGENTS.md), [agent architecture](CANON_GEO_AGENT_ARCHITECTURE.md), [inquiry roadmap](PLAN_CANON_GEO_AGENT_COMPLETE_INQUIRY.md), [Geo plan](PLAN_CANON_GEO.md) |

**Bottom line:** the original ambition was evidence that discriminates among answers. Recover that capability in one inspectable inquiry before expanding the machinery around it.
