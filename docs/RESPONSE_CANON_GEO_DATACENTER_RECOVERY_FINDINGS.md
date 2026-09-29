# Response to the recovery findings: what is actually broken, and the shortest path back

**Date:** 2026-09-29.  
**Status:** operator-requested response to the recovery field report. Documentation only. No Canon code, acceptance policy, release gate, or bead status is changed by this document.  
**Responds to:** [BRIEF_CANON_GEO_DATACENTER_RECOVERY_FINDINGS.md](BRIEF_CANON_GEO_DATACENTER_RECOVERY_FINDINGS.md).  
**Earlier context:** [RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md](RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md) and [BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md](BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md).  
**Reviewed repository baseline:** `1417ba27961e6a82d6efef667b37c389cca45136`. Re-pin before execution.

## 1. Bottom line

The recovery field report materially improves the diagnosis.

We should no longer describe the problem as one vague failure called "Canon Geo does not work." The evidence now supports at least two separate failure classes:

1. **Formulation failure:** the tested soft objective cannot disfavor a candidate. With positive `cost_if_absent` preferences and unrestricted set selection, adding members cannot worsen the score. The full supported universe therefore becomes an optimum by construction.
2. **Reach failure:** upstream universe construction can remove legitimate physical entities before the solver sees them. A fixed size threshold is not merely a performance optimization; it changes the semantic question by deciding which objects are allowed to exist in the answer.

These failures are independent. Better reach does not fix the objective. A better objective does not restore candidates that were removed upstream.

There is also a third issue that is not yet as fully measured but is important enough to treat as a design constraint:

3. **Evidence-lineage collapse:** multiple rows or providers can represent the same upstream observation. Source count is therefore not automatically corroboration. The recovery report's Pryor snapshot shows a concrete instance where Overture footprint records cite Microsoft as an upstream source.

The shortest path is **not** to expand acquisition, add more H3 coverage, tune weights, or finish the full inquiry roadmap. It is to run one narrowly scoped code experiment that introduces legitimate soft opposing evidence while keeping the full bounded neighborhood and lineage visible. That experiment should tell us whether the current member-selection representation can be salvaged for useful ranking or whether the decision variable has to move up one level to explicit competing site hypotheses.

The recommended order is:

1. stop silently dropping physical entities with a default size cutoff;
2. add one declared negative-preference primitive tied to actual contradictory evidence, not absence of support;
3. replay the same frozen cases without tuning after seeing results;
4. if discrimination improves only marginally, stop extending membership scoring and prototype explicit competing site interpretations.

Do not make Pryor "pick a winner" unless the evidence actually warrants one. The blind review says it does not.

---

## 2. What the field report has now established

### 2.1 The inclusion-only objective defect is confirmed

This is no longer a hypothesis.

The tested composition path uses a positive membership preference whose cost is incurred only when a preferred member is absent. For a selected set `S`:

```text
cost(S) = sum(weight_j for preferred member j not in S)
```

All weights are non-negative.

Therefore, if `S1` is a subset of `S2`:

```text
cost(S2) <= cost(S1)
```

Adding a member cannot make the soft score worse.

If all supported members can coexist structurally, the union of the supported members is an optimum. If every candidate receives some support, the whole universe is an optimum.

The recovery report reproduced this property on the pinned build, then replayed it over larger frozen Pryor universes. The result is important because it separates two questions that had been conflated:

- **Is the solver operating correctly?** Yes, relative to the request it receives.
- **Does the request encode the business question we intended?** No, not for this use.

This should be described as a formulation defect in the tested locator path, not as a numerical tuning issue and not as a failure of exact solving.

### 2.2 The ablation is especially decisive

The source ablation strengthens the mathematical diagnosis.

With each subset of the retained Pryor evidence, the optimum follows the same shape: include everything that the active evidence positively supports. No source can make a candidate less desirable. The evidence path can only answer:

> Which supported members are most expensive to omit?

It cannot answer:

> Which candidate interpretation fits the evidence better than its alternatives?

That distinction should govern the next patch.

### 2.3 Reach is a separate failure, not an implementation nuisance

The recovery report also shows that a size filter can eliminate legitimate site entities before inference.

The important insight is not the exact threshold value. It is that **minimum area is being used as a universe-definition rule without evidence that small objects are irrelevant to the requested claim**.

This means a rule introduced as retrieval simplification is actually making a semantic assertion:

> objects smaller than X are not members of the site worth considering.

That is unsafe as a default.

A bounded geographic neighborhood is a legitimate computational boundary. A size threshold may also be legitimate when it is itself part of the declared evidence or profile. But size should not silently stand in for membership.

This has a direct connection to the original H3 concept:

```text
rough point or bounded region
    -> H3/local spatial section
    -> surrounding physical scene
    -> evidence and relationships
    -> competing interpretations
    -> supported answer / ambiguity / contradiction
```

The tested path has instead looked more like:

```text
rough point
    -> H3/radius section
    -> hand-written filters delete objects
    -> hand-written rules convert observations into positive membership votes
    -> exact subset enumeration
```

The exact solver is downstream of two major pieces of implicit reasoning.

### 2.4 Pryor should remain ambiguous on the frozen evidence

This is an important correction to how the original field result could be interpreted.

The fact that the building nearest the official facility point ranked near the bottom as a singleton demonstrates the pathology of the objective. It does **not** establish that this singleton is the correct complete campus.

The blind review of the frozen Pryor evidence could not identify a defensible winner. That is useful evidence.

It means the recovery target should not be:

> make Pryor choose the nearest building.

It should be:

> make genuine supporting and opposing evidence affect competing interpretations correctly, while preserving ambiguity when the evidence is insufficient.

For an assurance-oriented tool, retained ambiguity is a successful result when the evidence cannot settle the question.

---

## 3. A deeper issue: members and alternatives are not the same variable

The recovery report correctly says that "alternatives and members are different variables." This deserves to be elevated because it may determine whether a narrow patch is enough.

Consider a neighborhood with five buildings:

```text
A — evidence supports target occupancy
B — evidence supports target occupancy
C — unknown
D — evidence supports competing occupant
E — unknown
```

A member-selection formulation asks:

> Which subset of A, B, C, D, E constitutes the target site?

A site-interpretation formulation asks:

> Which coherent real-world site hypothesis best explains the observations?

Those are not equivalent questions.

A site hypothesis could be something like:

```text
H1 = parcel P1 + buildings A/B + address X
H2 = parcel P2 + buildings C/D + address Y
H3 = wider campus assemblage P1/P2/P3 with uncertain extent
H4 = supplied address points to entrance/office but not complete physical extent
```

Evidence can then support, contradict, or fail to distinguish those hypotheses.

This has several advantages:

- contradictory occupancy evidence can lower one hypothesis without deleting it;
- uncertain campus membership does not have to be resolved before site identity is useful;
- the system can distinguish location/identity from complete extent;
- a source observation can attach to a relation or hypothesis rather than being forced into a per-member vote;
- multiple records with common lineage can remain one evidentiary fact.

The current member formulation may still be useful **inside** a site hypothesis for extent reasoning. The concern is using member inclusion as the first-level decision variable for identity.

The narrow experiment proposed below is useful precisely because it can tell us how urgent this redesign is.

---

## 4. Lineage: source agreement is not automatically independent evidence

The recovery report's lineage observation may be more important than its placement in the findings suggests.

The frozen Pryor neighborhood contained hundreds of footprint records that collapse to fewer physical entities, and many Overture rows identify Microsoft as an upstream source.

That means reasoning of the form:

```text
Microsoft footprint agrees
+
Overture footprint agrees
=
two independent confirmations
```

can be false.

The correct model is closer to:

```text
                 ┌─ Microsoft representation
physical object ─┤
                 └─ Overture representation
                       |
                       └─ may share Microsoft lineage
```

The two rows can improve data integration or geometry confidence without providing two independent votes about site identity.

Canon's architecture already says source count is provenance, not confidence. The field workflow needs to enforce that distinction earlier and more visibly.

A useful mental model for the H3 neighborhood is therefore **an evidence graph, not a bag of votes**:

```text
physical building
    |
    +-- footprint observation(s) -- lineage
    +-- parcel containment
    +-- address assertion
    +-- owner record
    +-- operator / occupant assertion
    +-- permit relation
    +-- morphology
    +-- adjacency / proximity
    +-- temporal observations

        ↓ interpreted under declared contracts

candidate site hypotheses

        ↓

support / opposition / unresolved relationships

        ↓

ranking + exact residual + acceptance + extent
```

The graph does not itself imply probabilistic inference. It simply preserves what an observation is about and where it came from before any scoring or hard admission occurs.

---

## 5. The next patch: add legitimate soft opposition, not arbitrary shrinking

The recovery report recommends a narrow Step 4 patch such as `prefer_absent`. I agree with using this as the next diagnostic experiment, with one important qualification:

> Treat it as an instrument to test the current representation, not as the presumed final architecture.

### 5.1 What the primitive should mean

A positive preference says:

> evidence supports member M being present in this interpretation.

A negative preference should say:

> evidence supports member M being absent from this interpretation.

The important word is **evidence**.

Examples that may justify a soft negative preference:

- an independent source names a competing occupant at the building;
- a parcel is affirmatively linked to another named project under a relevant temporal scope;
- a source explicitly identifies the building as belonging to a neighboring facility;
- a declared incompatible relation exists.

Examples that must **not** become negative preference by default:

- a provider did not return the building;
- a name was missing;
- a record is old;
- the candidate is small;
- the candidate is farther from the point;
- another source did not corroborate it.

Absence of support is not contradictory evidence.

### 5.2 Why this is preferable to an "extra member" penalty

An arbitrary complexity penalty would make smaller sets score better merely because they are smaller.

That would reverse the current bias without fixing the reasoning.

For example:

```text
old pathology: more members can only help
bad patch:     fewer members can only help
desired:       members help or hurt only when evidence says they should
```

The new primitive should therefore attach to a declared observation and relation with provenance.

### 5.3 Soft opposition must rank, not erase

A candidate with negative evidence must remain in the hard residual unless a separate sound hard constraint excludes it.

That preserves the architecture's core separation:

- hard evidence defines feasible worlds;
- soft evidence orders feasible worlds;
- acceptance policy decides whether an ordered result is usable;
- ambiguity remains when evidence cannot distinguish alternatives.

---

## 6. The controls matter more than the happy path

The patch should be judged against controls designed to catch fake improvement.

### Control A — no opposing evidence

Input contains only the current positive preference types.

Expected:

- byte-identical output to current behavior for unchanged request versions where compatibility requires it;
- no candidate is silently penalized because it lacks support;
- ambiguity remains ambiguity.

This proves the new primitive is not an ambient shrink rule.

### Control B — unrelated nearby object with explicit competing identity

Add a candidate with affirmative evidence that it belongs to another named occupant.

Expected:

- candidate remains hard-feasible;
- interpretations including it are ranked lower when the relation is applicable;
- evidence trace shows exactly why;
- removing the contradictory observation removes the penalty.

This proves negative evidence can actually discriminate.

### Control C — duplicate or common-lineage representation

Represent the same physical observation twice through sources that share upstream lineage.

Expected:

- no second independent vote merely because a second row/provider representation exists;
- lineage remains inspectable;
- deduplication or dependency semantics are deterministic.

This catches source-count inflation.

### Control D — true candidate receives contradictory soft evidence

Construct or retain a case where a correct candidate has some opposing evidence.

Expected:

- it is ranked lower where appropriate;
- it is not deleted from the feasible set;
- the explanation exposes the conflict.

This catches accidental conversion of ranking into exclusion.

### Control E — missing support

Candidate appears in the neighborhood but no source affirmatively supports or opposes it.

Expected:

- neutral unless the declared policy says otherwise;
- no penalty for mere absence.

This is critical.

### Control F — reach perturbation

Run the same case with the old size cutoff and with the complete bounded physical scene.

Expected:

- report candidate reach separately from ranking;
- never describe the smaller filtered universe as empirically complete unless evidence supports that claim.

---

## 7. How to judge the result on the three current cases

### Pryor

Desired outcome:

- **retained ambiguity** unless new valid opposing evidence genuinely narrows the alternatives;
- no artificial winner;
- any narrower preferred set must have inspectable positive or negative evidence;
- lineage-collapsed footprint duplication must not count as independent identity corroboration.

Pryor is a regression case for objective behavior, not a winner-selection benchmark.

### H1-P

This is the most useful current precision case because the report says reach is complete while much of the returned universe is non-site.

Desired outcome:

- unchanged hard reach;
- legitimate contradictory evidence lowers non-site interpretations when available;
- no tuning of weights after seeing the answer;
- unresolved label discrepancy is reported separately.

If the narrow negative primitive cannot improve H1-P because no contradictory observations exist, that is evidence about the limits of the member formulation.

### DEV-1

This case primarily exposes reach.

Desired outcome:

- first remove or neutralize the arbitrary size cutoff;
- evaluate whether physical entities re-enter the universe;
- then test ranking without conflating improved reach with improved precision.

A ranking patch should not receive credit for objects that were never reachable.

---

## 8. Decision rule after the narrow patch

The narrow patch is valuable because it creates a clean fork.

### Outcome 1 — substantial useful discrimination

If evidence-justified positive and negative member relations:

- reduce irrelevant preferred alternatives;
- preserve true candidates;
- retain justified ambiguity;
- work without ad hoc tuning;
- transfer across DEV-1 and H1-P;

then the member formulation may still be useful as an association layer.

Continue cautiously and measure.

### Outcome 2 — only marginal effect

This is plausible because competing occupants are not often explicitly named.

If the patch barely changes the answer, do not respond by:

- inventing more weights;
- turning missing support into opposition;
- adding global set-size penalties;
- adding more providers and counting them as votes;
- tightening geographic or size filters until the answer looks right.

Instead, conclude that the first-level decision variable is wrong for the task and prototype explicit site hypotheses.

### Outcome 3 — it improves apparent precision by suppressing truth

Stop. The negative evidence semantics are too aggressive or attached at the wrong grain/time.

Preserve the current exact residual and revise the translation.

---

## 9. Site hypotheses: what the larger redesign would look like if needed

Do not implement this before the narrow test unless the narrow primitive proves impossible to express cleanly. But define the direction now so the experiment has a meaningful alternative.

The first-level variable becomes something like:

```text
SiteHypothesis {
    id
    spatial_extent_claim
    address_claims
    parcel_membership_claims
    building_membership_claims
    operator_or_occupant_claims
    temporal_scope
    evidence_relations
}
```

A hypothesis does not need complete extent.

Examples:

```text
H1: "the supplied address refers to this parcel assemblage"
H2: "the supplied point is an entrance/POI for this neighboring campus"
H3: "these two parcels are the same operator campus but building extent is unresolved"
H4: "evidence is insufficient to distinguish H1 and H2"
```

Then:

- physical members remain lower-level variables;
- exact constraints govern compatibility and extent;
- evidence ranks or eliminates hypotheses only according to declared semantics;
- acceptance can occur at site/address grain while complete building membership remains unresolved.

This aligns closely with the existing architecture's separation of association support, exact consequence, acceptance, and complete extent.

---

## 10. What not to do next

### Do not expand H2/H3 discovery yet

Region-only anchor generation is necessary eventually, but it is not the shortest next move.

We already know the current inference path is malformed. Building more ways to feed cases into it increases surface area before the central representation question is settled.

After the narrow patch tells us whether the member formulation survives, implement the region-only entry point against the corrected inference path.

### Do not add more process machinery

The field report is candid that the recovery protocol created substantial pre-registration, sealed-label, manifest, and reviewer artifact weight.

That work was justified to isolate this defect.

It should not expand into another governance subsystem.

The next artifact should primarily be code, tests, and small deterministic replay outputs.

### Do not spend another cycle proving the inclusion-only theorem

It is established by the objective and reproduced empirically.

We do not need more cases to prove that a non-negative absence penalty rewards supersets.

### Do not treat source count as confidence

Especially where lineage overlaps.

### Do not tune toward labels

Weights and translation rules must be fixed before evaluation.

### Do not force a singleton

A correct ambiguous answer is better than a cosmetically precise wrong one.

---

## 11. Recommended implementation order

### Step A — preserve the full bounded physical neighborhood

For the diagnostic cases:

- use the declared geographic bound;
- retain all physically relevant objects the supported source adapters can return;
- remove the default minimum-size exclusion;
- preserve source-native identifiers, lineage, geometry, area, and relationships;
- report retrieval completeness and empirical reach separately.

This does not mean national-scale monolithic solving. H3/section blocking still bounds computation.

### Step B — introduce one declared soft-negative evidence kind

Name aside, semantics should be approximately:

```text
GeoSoftPreference {
    relation: prefer_present | prefer_absent
    member
    cost
    evidence_ref
}
```

or an equivalent generic representation consistent with the existing operator architecture.

Requirements:

- unsigned magnitude is fine if direction is explicit;
- every negative preference has evidence provenance;
- no fallback from "not observed" to "prefer absent";
- old requests behave identically;
- canonical serialization remains deterministic.

### Step C — update translation only where evidence warrants it

Do not globally infer negative evidence.

Create one or two explicit translators for observed contradictory relations, such as competing named occupant evidence.

### Step D — run the frozen cases without retuning

Run:

1. Pryor;
2. H1-P;
3. DEV-1 after reach correction.

Record:

- reach before/after;
- hard residual;
- soft ranking before/after;
- which observations caused rank changes;
- lineage dependencies;
- whether the correct labeled entity remained reachable;
- whether ambiguity was preserved when appropriate.

### Step E — decide whether to retain member-level ranking

Use section 8's decision rule.

Only then decide whether to:

- continue extending member evidence semantics, or
- build explicit competing site hypotheses.

### Step F — only after that, return to region-only discovery

Then implement the H2/H3 anchor-generation workflow against the chosen inference model.

---

## 12. Acceptance criteria for getting "back on track"

The tool is not back on track merely because one output is smaller.

For this recovery milestone, require all of the following:

1. **Bounded scene integrity:** retrieval no longer silently excludes small physical objects absent a declared evidence-based reason.
2. **Directional evidence:** the ranking system can express both legitimate support and legitimate opposition.
3. **No negative-by-absence:** missing corroboration is neutral unless a declared evidence contract says otherwise.
4. **Hard/soft separation:** opposing soft evidence never silently excludes a hard-feasible candidate.
5. **Lineage awareness:** duplicated/common-upstream observations do not gain independent evidentiary weight by representation count.
6. **Pryor honesty:** the system may remain ambiguous; no singleton is required.
7. **Transfer:** the rule is fixed before scoring and replayed on more than one labeled case.
8. **Inspectability:** every ranking change can be traced to specific observations and relations.
9. **Backward determinism:** requests without the new semantics remain byte-identical where the contract requires it.
10. **Architecture decision:** the experiment produces enough evidence to decide whether member-selection remains viable or site hypotheses become the first-level representation.

---

## 13. Interpretation of the recovery report's five operator decisions

The field report asks five decisions. Recommended answers:

### 13.1 Approve Step 4?

**Yes, as a diagnostic narrow patch, not as a commitment to the final formulation.**

Pair it with the reach correction so the patch is not evaluated on a universe known to omit legitimate entities.

### 13.2 Require an independent parcel-grain DEV-1 label before patching?

**No for this diagnostic experiment.**

The label limitation prevents an accuracy claim, but it does not block testing objective behavior, reach, ranking direction, and evidence traceability. Continue to disclose the limitation.

Acquire better independent parcel truth before using DEV-1 in a formal accuracy gate.

### 13.3 Run H2/H3 now?

**Not yet.**

Preserve them as the next discovery tests, but do not build anchor generation before the inference model is corrected enough to justify expanding the workflow.

### 13.4 Repeat kernel replay with a fresh blind agent before Step 4?

**No.**

The objective property is mathematically established and reproduced. Additional blindness does not change the implementation decision. Use fresh-agent reproduction after the patch, where agent usability becomes relevant again.

### 13.5 Reconcile H1-P now?

**Only enough to avoid scoring a known inconsistent label as truth.**

Do not make reconciliation a prerequisite for implementing the generic patch. Resolve it before claiming H1-P accuracy or before using it as a release gate.

---

## 14. Final position

The recovery report makes the Canon Geo direction more salvageable, not less.

The experiments do **not** show that constraint reasoning is inappropriate for geographic association.

They show that the tested workflow does not yet present the solver with the geographic inference problem we intended.

The current path:

- partially filters the local physical scene using assumptions that can affect truth reach;
- collapses rich observations into mostly one-directional inclusion preferences;
- can accidentally treat common-lineage representations as corroboration;
- enumerates subsets exactly;
- then reports honest ambiguity about the resulting formulation.

The exactness is useful. The formulation is the problem.

The immediate recovery should therefore preserve the parts that are already valuable:

- deterministic bounded computation;
- provenance;
- exact residuals;
- typed ambiguity and conflict;
- review/acceptance separation;

while fixing the semantics at the evidence-to-hypothesis boundary.

The next experiment should be deliberately small:

> **Full bounded neighborhood -> preserved physical entities and lineage -> genuine positive and opposing evidence -> same frozen cases -> no post-label tuning.**

If this produces useful discrimination, continue with the member formulation carefully.

If it does not, stop patching subset scores and move the first-level decision variable to explicit competing site interpretations.

That test should tell us which Canon Geo architecture we actually need.
